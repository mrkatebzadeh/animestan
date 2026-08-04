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
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ORIGIN, REFERER, USER_AGENT};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use spdlog::prelude::*;
use std::collections::HashMap;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::{
    CoreResult,
    config::AppConfig,
    error::Error,
    fixtures,
    models::{AnimeEntry, Episode, StreamLink},
    source::SourceDefinition,
};

mod allanime;
pub(crate) mod anidb;

use crate::client::allanime::{
    ALLANIME_EPISODE_EMBED_GQL, ALLANIME_EPISODE_EMBED_PERSISTED_HASH, ALLANIME_EPISODES_GQL,
    ALLANIME_REFERER, ALLANIME_SEARCH_GQL, ALLANIME_TRANSLATION, AllAnimeEpisodeEmbedResponse,
    AllAnimeEpisodesResponse, AllAnimeSearchResponse, AllAnimeSourceUrl, build_aa_req,
    build_embed_url, build_graphql_url, decode_source_url, fetch_allanime_key_material,
    maybe_decrypt_response_data, ordered_source_urls, parse_episode_number, select_stream_url,
    split_episode_id,
};

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
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(anidb::USER_AGENT_VALUE),
        );
        headers.insert(REFERER, HeaderValue::from_static(ALLANIME_REFERER));

        let client = BlockingHttpClient::builder()
            .default_headers(headers)
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

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: request.url.to_string(),
                status: response.status().as_u16(),
            }
            .into());
        }

        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: request.url.to_string(),
            source,
        })?;
        Ok(body)
    }
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
        Ok(Self { source, fetcher })
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

        if use_fixtures {
            let source = Self::select_fixture_source(config)?;
            let fetcher = FetchBackend::fixtures()?;
            Ok(Self { source, fetcher })
        } else {
            let source = SourceDefinition::anidb();
            let fetcher = FetchBackend::http()?;
            Ok(Self { source, fetcher })
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
        Self { source, fetcher }
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
        } else if self.uses_allanime() {
            self.search_allanime(query)?
        } else {
            let url = self.source.search.render(&[("query", query)])?;
            let request = FetchRequest::get(url.clone());
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
        } else if self.uses_allanime() {
            self.list_episodes_allanime(anime_id)?
        } else {
            let url = self.source.episodes.render(&[("anime_id", anime_id)])?;
            let request = FetchRequest::get(url.clone());
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

        let link = if self.uses_allanime() {
            self.resolve_stream_url_allanime(episode_id)?
        } else {
            let url = self.source.stream.render(&[("episode_id", episode_id)])?;
            let request = FetchRequest::get(url.clone());
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

    fn search_anidb(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
        let url = self.source.search.render(&[("query", query)])?;
        let body = self.fetcher.fetch(&FetchRequest::get(url))?;
        anidb::parse_search(&body, &self.source.id)
    }

    fn list_episodes_anidb(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        let numeric_id = anidb::anime_numeric_id(anime_id)?;
        let url = self.source.episodes.render(&[("anime_id", numeric_id)])?;
        let body = self.fetcher.fetch(&FetchRequest::get(url))?;
        anidb::parse_episodes(&body, anime_id, &self.source.id)
    }

    fn search_allanime(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
        let variables = json!({
            "search": {
                "allowAdult": false,
                "allowUnknown": false,
                "query": query,
            },
            "limit": 40,
            "page": 1,
            "translationType": ALLANIME_TRANSLATION,
            "countryOrigin": "ALL",
        });
        let request = Self::post_graphql_request(ALLANIME_SEARCH_GQL, &variables);
        let payload: AllAnimeSearchResponse = self.fetch_and_parse(&request)?;

        let entries = payload
            .data
            .shows
            .edges
            .into_iter()
            .map(|edge| AnimeEntry {
                id: edge.id,
                title: edge.name,
                source_id: self.source.id.clone(),
            })
            .collect();

        Ok(entries)
    }

    fn list_episodes_allanime(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        let variables = json!({ "showId": anime_id });
        let request = Self::post_graphql_request(ALLANIME_EPISODES_GQL, &variables);
        let payload: AllAnimeEpisodesResponse = self.fetch_and_parse(&request)?;

        let Some(show) = payload.data.show else {
            return Ok(Vec::new());
        };

        let mut episodes: Vec<Episode> = show
            .available_episodes_detail
            .sub
            .into_iter()
            .map(|episode_string| Episode {
                id: format!("{}:{}", show.id, episode_string),
                number: parse_episode_number(&episode_string),
                title: format!("Episode {episode_string}"),
                anime_id: show.id.clone(),
                source_id: self.source.id.clone(),
                synopsis: None,
                duration_secs: None,
                air_date: None,
            })
            .collect();

        episodes.sort_by_key(|episode| episode.number);
        Ok(episodes)
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_stream_url_allanime(&self, episode_id: &str) -> CoreResult<StreamLink> {
        let (show_id, episode_string) = split_episode_id(episode_id)?;
        let keys = fetch_allanime_key_material()?;
        let (payload, request_url_for_errors) =
            self.fetch_allanime_episode_embed(&show_id, &episode_string, &keys)?;

        let episode = payload
            .data
            .episode
            .ok_or_else(|| Error::StreamResolution {
                message: format!("episode metadata missing (request: {request_url_for_errors})"),
            })?;

        let sources = ordered_source_urls(&episode.source_urls);
        if sources.is_empty() {
            return Err(Error::StreamResolution {
                message: "no source URLs returned".to_string(),
            }
            .into());
        }

        let mut last_error: Option<anyhow::Error> = None;
        for source in sources {
            match self.resolve_stream_from_allanime_source(episode_id, source) {
                Ok(link) => return Ok(link),
                Err(err) => last_error = Some(err),
            }
        }

        let last_error_text =
            last_error.map_or_else(|| "no specific error".to_string(), |err| err.to_string());

        Err(Error::StreamResolution {
            message: format!("all AllAnime source URLs failed; last error: {last_error_text}"),
        }
        .into())
    }

    fn fetch_allanime_episode_embed(
        &self,
        show_id: &str,
        episode_string: &str,
        keys: &crate::client::allanime::AllAnimeKeyMaterial,
    ) -> CoreResult<(AllAnimeEpisodeEmbedResponse, Url)> {
        let now_ms = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|source| Error::StreamResolution {
                    message: format!("system clock before unix epoch: {source}"),
                })?
                .as_millis(),
        )
        .map_err(|_| Error::StreamResolution {
            message: "system clock too far in the future".to_string(),
        })?;

        let variables = json!({
            "showId": show_id,
            "translationType": ALLANIME_TRANSLATION,
            "episodeString": episode_string,
        });
        let variables_json = format!(
            "{{\"showId\":{},\"translationType\":{},\"episodeString\":{}}}",
            serde_json::to_string(show_id).map_err(|source| Error::ResponseParse {
                url: ALLANIME_EPISODE_EMBED_GQL.to_string(),
                source,
            })?,
            serde_json::to_string(ALLANIME_TRANSLATION).map_err(|source| Error::ResponseParse {
                url: ALLANIME_EPISODE_EMBED_GQL.to_string(),
                source,
            })?,
            serde_json::to_string(episode_string).map_err(|source| Error::ResponseParse {
                url: ALLANIME_EPISODE_EMBED_GQL.to_string(),
                source,
            })?,
        );
        let aa_req = build_aa_req(
            ALLANIME_EPISODE_EMBED_PERSISTED_HASH,
            &keys.build_id,
            keys.epoch,
            &keys.content_lane,
            &keys.key,
            now_ms,
        )?;
        let extensions_json = format!(
            "{{\"persistedQuery\":{{\"version\":1,\"sha256Hash\":\"{}\"}},\"k\":\"{}\",\"aaReq\":{}}}",
            ALLANIME_EPISODE_EMBED_PERSISTED_HASH,
            keys.content_lane.as_str(),
            serde_json::to_string(&aa_req).map_err(|source| Error::ResponseParse {
                url: ALLANIME_EPISODE_EMBED_GQL.to_string(),
                source,
            })?,
        );

        let mut persisted_url = build_graphql_url(ALLANIME_EPISODE_EMBED_GQL, &variables);
        {
            let mut pairs = persisted_url.query_pairs_mut();
            pairs.append_pair("variables", &variables_json);
            pairs.append_pair("extensions", &extensions_json);
        }

        let persisted_request = FetchRequest::get(persisted_url)
            .with_header(
                HeaderName::from_static("x-build-id"),
                HeaderValue::from_str(&keys.build_id).expect("valid AllAnime build ID"),
            )
            .with_header(REFERER, HeaderValue::from_static(ALLANIME_REFERER))
            .with_header(ORIGIN, HeaderValue::from_static(ALLANIME_REFERER));

        let decode_payload = |value: Value,
                              request_url: &Url,
                              key: &[u8; 32]|
         -> CoreResult<AllAnimeEpisodeEmbedResponse> {
            let value = maybe_decrypt_response_data(value, key)?;
            serde_json::from_value::<AllAnimeEpisodeEmbedResponse>(value).map_err(|source| {
                Error::ResponseParse {
                    url: request_url.to_string(),
                    source,
                }
                .into()
            })
        };

        let value: Value = self.fetch_and_parse(&persisted_request)?;
        let payload = decode_payload(value, &persisted_request.url, &keys.key)?;
        Ok((payload, persisted_request.url.clone()))
    }

    fn resolve_stream_from_allanime_source(
        &self,
        episode_id: &str,
        source: &AllAnimeSourceUrl,
    ) -> CoreResult<StreamLink> {
        let decoded = decode_source_url(&source.source_url)?;
        if (decoded.starts_with("http://") || decoded.starts_with("https://"))
            && !decoded.contains("/clock")
        {
            let url = Url::parse(&decoded).map_err(|source| Error::StreamUrlParse {
                url: decoded.clone(),
                source,
            })?;

            return Ok(StreamLink {
                url,
                episode_id: episode_id.to_string(),
                source_id: self.source.id.clone(),
            });
        }

        let embed_url = build_embed_url(&decoded)?;
        let embed_request = FetchRequest::get(embed_url);
        let payload: Value = self.fetch_and_parse(&embed_request)?;
        let stream_url = select_stream_url(&payload)?;
        let url = Url::parse(&stream_url).map_err(|source| Error::StreamUrlParse {
            url: stream_url.clone(),
            source,
        })?;

        Ok(StreamLink {
            url,
            episode_id: episode_id.to_string(),
            source_id: self.source.id.clone(),
        })
    }

    fn log_result_count(action: &str, subject: &str, count: usize) {
        if count == 0 {
            warn!("{action} returned no results for '{subject}'");
        } else {
            debug!("{action} returned {count} results for '{subject}'");
        }
    }

    fn post_graphql_request(query: &str, variables: &Value) -> FetchRequest {
        let url = build_graphql_url(query, variables);
        let body = json!({
            "query": query,
            "variables": variables,
        });
        let request = FetchRequest::post(url, body);
        request
            .with_header(REFERER, HeaderValue::from_static(ALLANIME_REFERER))
            .with_header(ORIGIN, HeaderValue::from_static(ALLANIME_REFERER))
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

    fn uses_allanime(&self) -> bool {
        self.source.id == SourceDefinition::ALLANIME_ID
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
    fn allanime_graphql_posts_use_mkissa_origin() {
        let request = AnimeClient::<FixtureFetcher>::post_graphql_request(
            ALLANIME_SEARCH_GQL,
            &json!({"search": {"query": "naruto"}}),
        );

        assert_eq!(
            request.headers.get(ORIGIN).unwrap().to_str().unwrap(),
            "https://mkissa.to"
        );
        assert_eq!(
            request.headers.get(REFERER).unwrap().to_str().unwrap(),
            "https://mkissa.to"
        );
        assert!(request.headers.get("x-build-id").is_none());
    }
}
