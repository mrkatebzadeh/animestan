// Copyright (C) 2026 M.R. Siavash Katebzadeh <mr@katebzadeh.xyz>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use reqwest::Method;
use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use spdlog::prelude::*;
use std::collections::HashMap;
use std::env;
use url::Url;

use crate::{
    CoreResult,
    config::{AppConfig, QualityPreference, StreamingMode},
    error::Error,
    fixtures,
    models::{AnimeEntry, Episode, StreamLink},
    source::SourceDefinition,
};

pub(crate) mod anidb;

const FIXTURES_ENV: &str = "ANIMESTAN_USE_FIXTURES";

fn fixtures_fetch_enabled() -> bool {
    env::var(FIXTURES_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[derive(Debug)]
pub struct FetchRequest {
    url: Url,
    method: Method,
    body: Option<Value>,
    headers: HeaderMap,
}

impl FetchRequest {
    pub fn get(url: Url) -> Self {
        Self {
            url,
            method: Method::GET,
            body: None,
            headers: HeaderMap::new(),
        }
    }

    pub fn post(url: Url, body: Value) -> Self {
        Self {
            url,
            method: Method::POST,
            body: Some(body),
            headers: HeaderMap::new(),
        }
    }

    pub fn with_header(mut self, name: reqwest::header::HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn key(&self) -> String {
        let mut key = format!("{}|{}", self.method.as_str(), self.url);
        if let Some(body) = &self.body {
            key.push('|');
            key.push_str(&body.to_string());
        }
        key
    }
}

pub trait Fetcher {
    /// Fetches the response body located at `url`.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the request cannot be fulfilled or the response
    /// body cannot be retrieved.
    fn fetch(&self, request: &FetchRequest) -> CoreResult<String>;
}

pub struct FixtureFetcher {
    responses: HashMap<String, String>,
}

impl FixtureFetcher {
    /// Creates a [`FixtureFetcher`] preloaded with static responses.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the fixture response data cannot be loaded
    /// from disk.
    pub fn new() -> CoreResult<Self> {
        let responses = fixtures::load_responses()?;
        Ok(Self { responses })
    }
}

impl Fetcher for FixtureFetcher {
    fn fetch(&self, request: &FetchRequest) -> CoreResult<String> {
        let key = request.key();
        let value = self
            .responses
            .get(&key)
            .ok_or_else(|| Error::MissingFixture {
                url: request.url.to_string(),
            })?;
        Ok(value.clone())
    }
}

pub struct HttpFetcher {
    client: BlockingHttpClient,
}

impl HttpFetcher {
    fn new() -> CoreResult<Self> {
        let client = BlockingHttpClient::builder()
            .redirect(safe_redirect_policy())
            .build()
            .map_err(Error::HttpClient)?;

        Ok(Self { client })
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, request: &FetchRequest) -> CoreResult<String> {
        let mut builder = self
            .client
            .request(request.method.clone(), request.url.clone());

        if !request.headers.is_empty() {
            builder = builder.headers(request.headers.clone());
        }

        if let Some(body) = request.body.as_ref() {
            builder = builder.json(body);
        }

        let response = builder.send().map_err(|source| Error::HttpRequest {
            url: request.url.to_string(),
            source,
        })?;
        let status = response.status();
        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: request.url.to_string(),
            source,
        })?;

        if !status.is_success() {
            return Err(http_status_error(&request.url, status.as_u16(), &body).into());
        }

        Ok(body)
    }
}

pub(crate) fn http_status_error(url: &Url, status: u16, body: &str) -> Error {
    if status == 403 && is_anidb_url(url) && body.contains("Just a moment") {
        Error::ProviderBlocked {
            url: url.to_string(),
        }
    } else {
        Error::HttpStatus {
            url: url.to_string(),
            status,
        }
    }
}

pub(crate) fn safe_redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        let Some(previous) = attempt.previous().last() else {
            return attempt.stop();
        };
        if same_origin(previous, attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn is_anidb_url(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some("anidb.app")
}

pub enum FetchBackend {
    Fixtures(FixtureFetcher),
    Http(HttpFetcher),
}

impl FetchBackend {
    fn fixtures() -> CoreResult<Self> {
        Ok(Self::Fixtures(FixtureFetcher::new()?))
    }

    fn http() -> CoreResult<Self> {
        Ok(Self::Http(HttpFetcher::new()?))
    }
}

impl Fetcher for FetchBackend {
    fn fetch(&self, request: &FetchRequest) -> CoreResult<String> {
        match self {
            Self::Fixtures(fetcher) => fetcher.fetch(request),
            Self::Http(fetcher) => fetcher.fetch(request),
        }
    }
}

pub struct AnimeClient<F: Fetcher> {
    source: SourceDefinition,
    fetcher: F,
    mode: StreamingMode,
    quality: QualityPreference,
}

impl AnimeClient<FixtureFetcher> {
    /// Builds an [`AnimeClient`] backed by fixture data for offline testing.
    ///
    /// # Errors
    ///
    /// Returns an error if the catalog, default source, or fixture responses cannot be
    /// loaded.
    pub fn with_fixtures() -> CoreResult<Self> {
        let catalog = fixtures::load_catalog()?;
        let source = catalog.default_source()?;
        let fetcher = FixtureFetcher::new()?;
        Ok(Self {
            source,
            fetcher,
            mode: StreamingMode::Sub,
            quality: QualityPreference::Best,
        })
    }
}

impl AnimeClient<FetchBackend> {
    /// Builds an [`AnimeClient`] backed by fixtures or live HTTP based on the environment.
    ///
    /// # Errors
    ///
    /// Returns an error if fixture data cannot be loaded or the HTTP client cannot be
    /// constructed.
    pub fn with_env() -> CoreResult<Self> {
        let config = AppConfig::load_default()?;
        Self::from_config(&config)
    }

    /// Builds an [`AnimeClient`] from the provided configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if fixture data must be loaded but is unavailable or malformed, or the
    /// HTTP client cannot be constructed.
    pub fn from_config(config: &AppConfig) -> CoreResult<Self> {
        let use_fixtures = config.use_fixtures.unwrap_or_else(fixtures_fetch_enabled);
        let mode = StreamingMode::parse(config.mode.as_deref())?;
        let quality = QualityPreference::parse(config.quality.as_deref())?;

        if use_fixtures {
            let source = Self::select_fixture_source(config)?;
            let fetcher = FetchBackend::fixtures()?;
            Ok(Self {
                source,
                fetcher,
                mode,
                quality,
            })
        } else {
            let source = SourceDefinition::anidb();
            let fetcher = FetchBackend::http()?;
            Ok(Self {
                source,
                fetcher,
                mode,
                quality,
            })
        }
    }

    fn select_fixture_source(config: &AppConfig) -> CoreResult<SourceDefinition> {
        let catalog = fixtures::load_catalog()?;
        if let Some(source_id) = config.source_id.as_deref() {
            catalog
                .source_by_id(source_id)
                .ok_or_else(|| Error::UnknownSourceId {
                    source_id: source_id.to_string(),
                })
                .map_err(Into::into)
        } else {
            catalog.default_source()
        }
    }
}

impl<F: Fetcher> AnimeClient<F> {
    pub fn new(source: SourceDefinition, fetcher: F) -> Self {
        Self {
            source,
            fetcher,
            mode: StreamingMode::Sub,
            quality: QualityPreference::Best,
        }
    }

    /// Fetches anime entries that match the provided `query`.
    ///
    /// # Errors
    ///
    /// Returns an error if the search URL cannot be rendered, the fetcher fails to retrieve
    /// the response body, or the payload cannot be parsed into [`AnimeEntry`] values.
    pub fn search(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
        info!("searching query '{query}' via {}", self.source.id);

        let entries = if self.uses_anidb() {
            self.search_anidb(query)?
        } else {
            let url = self.source.search.render(&[("query", query)])?;
            let request = self.get_request(url);
            let mut entries: Vec<AnimeEntry> = self.fetch_and_parse(&request)?;
            for entry in &mut entries {
                entry.source_id.clone_from(&self.source.id);
            }
            entries
        };

        Self::log_result_count("search", query, entries.len());
        Ok(entries)
    }

    /// Lists the episodes corresponding to the provided `anime_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the episodes URL cannot be rendered, the fetcher fails to retrieve
    /// the response body, or the payload cannot be parsed into [`Episode`] values.
    pub fn list_episodes(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        info!("listing episodes for '{anime_id}' via {}", self.source.id);

        let episodes = if self.uses_anidb() {
            self.list_episodes_anidb(anime_id)?
        } else {
            let url = self.source.episodes.render(&[("anime_id", anime_id)])?;
            let request = self.get_request(url);
            let mut episodes: Vec<Episode> = self.fetch_and_parse(&request)?;
            for episode in &mut episodes {
                episode.source_id.clone_from(&self.source.id);
            }
            episodes
        };

        Self::log_result_count("episode listing", anime_id, episodes.len());
        Ok(episodes)
    }

    /// Resolves a direct stream link for the episode identified by `episode_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream URL cannot be rendered, the fetcher fails to retrieve
    /// JSON, the payload cannot be parsed, or the returned URL is invalid.
    pub fn resolve_stream_url(&self, episode_id: &str) -> CoreResult<StreamLink> {
        info!("resolving stream for '{episode_id}' via {}", self.source.id);

        let link = if self.uses_anidb() {
            self.resolve_stream_url_anidb(episode_id)?
        } else {
            let url = self.source.stream.render(&[("episode_id", episode_id)])?;
            let request = self.get_request(url);
            let payload: StreamPayload = self.fetch_and_parse(&request)?;
            let stream_url = Url::parse(&payload.url).map_err(|source| Error::StreamUrlParse {
                url: payload.url.clone(),
                source,
            })?;

            StreamLink {
                url: stream_url,
                episode_id: episode_id.to_owned(),
                source_id: self.source.id.clone(),
            }
        };

        debug!("resolved stream for '{episode_id}' to {}", link.url);
        Ok(link)
    }

    fn resolve_stream_url_anidb(&self, episode_id: &str) -> CoreResult<StreamLink> {
        anidb::validate_episode_id(episode_id)?;
        let languages_url = self.source.stream.render(&[("episode_id", episode_id)])?;
        let languages = self.fetcher.fetch(&self.get_request(languages_url))?;
        let embed_url = anidb::parse_languages(&languages, self.mode)?;
        let embed = self.fetcher.fetch(&self.get_request(embed_url))?;
        let master_url = anidb::extract_master_url(&embed)?;
        let master = self.fetcher.fetch(&self.get_request(master_url.clone()))?;
        let variants = anidb::parse_master_playlist(&master, &master_url)?;
        let selected = anidb::select_variant(&variants, self.quality)?;
        Ok(StreamLink {
            url: selected.url.clone(),
            episode_id: episode_id.to_string(),
            source_id: self.source.id.clone(),
        })
    }

    fn search_anidb(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
        let url = self.source.search.render(&[("query", query)])?;
        let body = self.fetcher.fetch(&self.get_request(url))?;
        anidb::parse_search(&body, &self.source.id)
    }

    fn list_episodes_anidb(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        let numeric_id = anidb::anime_numeric_id(anime_id)?;
        let url = self.source.episodes.render(&[("anime_id", numeric_id)])?;
        let body = self.fetcher.fetch(&self.get_request(url))?;
        anidb::parse_episodes(&body, anime_id, &self.source.id)
    }

    fn log_result_count(action: &str, subject: &str, count: usize) {
        if count == 0 {
            warn!("{action} returned no results for '{subject}'");
        } else {
            debug!("{action} returned {count} results for '{subject}'");
        }
    }

    fn get_request(&self, url: Url) -> FetchRequest {
        let request = FetchRequest::get(url);
        if self.uses_anidb() && is_anidb_url(&request.url) {
            request
                .with_header(
                    USER_AGENT,
                    HeaderValue::from_static(anidb::USER_AGENT_VALUE),
                )
                .with_header(REFERER, HeaderValue::from_static(anidb::BASE_URL))
        } else {
            request
        }
    }

    fn fetch_and_parse<T>(&self, request: &FetchRequest) -> CoreResult<T>
    where
        T: DeserializeOwned,
    {
        let body = self.fetcher.fetch(request)?;
        let parsed = serde_json::from_str(&body).map_err(|source| Error::ResponseParse {
            url: request.url.to_string(),
            source,
        })?;
        Ok(parsed)
    }

    fn uses_anidb(&self) -> bool {
        self.source.id == SourceDefinition::ANIDB_ID
    }
}

#[derive(Debug, Deserialize)]
struct StreamPayload {
    url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::ORIGIN;
    use std::cell::{Cell, RefCell};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::rc::Rc;
    use std::time::Duration;

    struct CapturingFetcher {
        headers: Rc<RefCell<Option<HeaderMap>>>,
    }

    impl Fetcher for CapturingFetcher {
        fn fetch(&self, request: &FetchRequest) -> CoreResult<String> {
            self.headers.replace(Some(request.headers.clone()));
            Ok(r#"<a href="/anime/naruto-3686" title="Naruto"></a>"#.to_string())
        }
    }

    struct CountingFetcher {
        calls: Rc<Cell<usize>>,
    }

    impl Fetcher for CountingFetcher {
        fn fetch(&self, _request: &FetchRequest) -> CoreResult<String> {
            self.calls.set(self.calls.get() + 1);
            Ok(String::new())
        }
    }

    #[test]
    fn search_returns_results() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let results = client.search("naruto").expect("search results");
        assert!(results.iter().any(|entry| entry.id == "naruto-3686"));
    }

    #[test]
    fn list_episodes_returns_entries() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let episodes = client
            .list_episodes("naruto-3686")
            .expect("episode listing");
        assert!(episodes.iter().any(|episode| episode.id == "6087"));
    }

    #[test]
    fn resolve_stream_url_returns_selected_anidb_variant() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let stream = client
            .resolve_stream_url("6087")
            .expect("stream resolution");
        assert_eq!(
            stream.url.as_str(),
            "https://stream.example/naruto/1080/index.m3u8"
        );
    }

    #[test]
    fn from_config_applies_stream_preferences() {
        let config = AppConfig {
            use_fixtures: Some(true),
            mode: Some("dub".to_string()),
            quality: Some("720p".to_string()),
            ..AppConfig::default()
        };
        let client = AnimeClient::from_config(&config).expect("configured fixtures client");
        assert_eq!(client.mode, StreamingMode::Dub);
        assert_eq!(client.quality, QualityPreference::Height(720));
    }

    #[test]
    fn anidb_requests_use_anidb_headers() {
        let headers = Rc::new(RefCell::new(None));
        let client = AnimeClient::new(
            SourceDefinition::anidb(),
            CapturingFetcher {
                headers: Rc::clone(&headers),
            },
        );

        client.search("naruto").expect("search results");

        let headers = headers.borrow();
        let headers = headers.as_ref().expect("captured request headers");
        assert_eq!(
            headers
                .get(USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some(anidb::USER_AGENT_VALUE)
        );
        assert_eq!(
            headers.get(REFERER).and_then(|value| value.to_str().ok()),
            Some(anidb::BASE_URL)
        );
        assert!(headers.get(ORIGIN).is_none());
    }

    #[test]
    fn cloudflare_search_status_uses_response_body() {
        let url = Url::parse("https://anidb.app/browse?q=naruto").unwrap();
        let error = super::http_status_error(&url, 403, "<title>Just a moment...</title>");

        assert!(
            matches!(error, Error::ProviderBlocked { url: error_url } if error_url == url.to_string())
        );
    }

    #[test]
    fn cloudflare_detail_status_uses_response_body() {
        let url = Url::parse("https://anidb.app/anime/naruto-3686").unwrap();
        let error = super::http_status_error(&url, 403, "<title>Just a moment...</title>");

        assert!(
            matches!(error, Error::ProviderBlocked { url: error_url } if error_url == url.to_string())
        );
    }

    #[test]
    fn non_403_status_never_reports_provider_blocked() {
        let url = Url::parse("https://anidb.app/browse?q=naruto").unwrap();
        let error = super::http_status_error(&url, 500, "<title>Just a moment...</title>");

        assert!(
            matches!(error, Error::HttpStatus { url: error_url, status } if error_url == url.to_string() && status == 500)
        );
    }

    #[test]
    fn redirect_policy_only_follows_same_origin() {
        let anidb = Url::parse("https://anidb.app/browse").unwrap();
        let anidb_next = Url::parse("https://anidb.app/anime/naruto-3686").unwrap();
        let external = Url::parse("https://cdn.example/master.m3u8").unwrap();

        assert!(super::same_origin(&anidb, &anidb_next));
        assert!(!super::same_origin(&anidb, &external));
    }

    #[test]
    fn redirect_policy_stops_before_cross_origin_request() {
        let origin_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin_address = origin_listener.local_addr().unwrap();
        let external_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        external_listener.set_nonblocking(true).unwrap();
        let external_address = external_listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = origin_listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{external_address}/media\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = BlockingHttpClient::builder()
            .redirect(super::safe_redirect_policy())
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        let response = client
            .get(format!("http://{origin_address}/start"))
            .send()
            .unwrap();

        server.join().unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert!(matches!(
            external_listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn invalid_anidb_episode_id_does_not_fetch() {
        let calls = Rc::new(Cell::new(0));
        let client = AnimeClient::new(
            SourceDefinition::anidb(),
            CountingFetcher {
                calls: Rc::clone(&calls),
            },
        );

        let error = client
            .resolve_stream_url("episode/1")
            .expect_err("invalid AniDB episode id");

        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::InvalidEpisodeId { episode_id }) if episode_id == "episode/1"
        ));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn external_media_requests_omit_anidb_headers() {
        let client = AnimeClient::new(
            SourceDefinition::anidb(),
            CapturingFetcher {
                headers: Rc::new(RefCell::new(None)),
            },
        );
        let request = client.get_request(Url::parse("https://stream.example/master.m3u8").unwrap());

        assert!(request.headers.get(USER_AGENT).is_none());
        assert!(request.headers.get(REFERER).is_none());
        assert!(request.headers.get(ORIGIN).is_none());
    }
}
