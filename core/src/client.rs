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

use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use spdlog::prelude::*;
use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

use crate::{
    CoreResult,
    config::{AppConfig, QualityPreference, StreamingMode},
    error::Error,
    fixtures,
    models::{AnimeEntry, Episode, StreamLink},
};

pub(crate) mod anidb;

const FIXTURES_ENV: &str = "ANIMESTAN_USE_FIXTURES";
const CURL_EXECUTABLES: [&str; 5] = [
    "curl_firefox135",
    "curl_chrome136",
    "curl_chrome116",
    "curl_ff117",
    "curl",
];
#[cfg(target_os = "macos")]
const CURL_CIPHERS: &str = "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305";
#[cfg(target_os = "macos")]
const CURL_TLS13_CIPHERS: &str =
    "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256";
const CURL_STATUS_FORMAT: &str = "%{stderr}%{http_code}";

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
    headers: HeaderMap,
}

impl FetchRequest {
    pub fn get(url: Url) -> Self {
        Self {
            url,
            headers: HeaderMap::new(),
        }
    }

    pub fn with_header(mut self, name: reqwest::header::HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn key(&self) -> String {
        format!("GET|{}", self.url)
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
    curl_executable: Option<PathBuf>,
}

impl HttpFetcher {
    fn new() -> CoreResult<Self> {
        let client = BlockingHttpClient::builder()
            .redirect(safe_redirect_policy())
            .build()
            .map_err(Error::HttpClient)?;

        Ok(Self {
            client,
            curl_executable: select_curl_executable(),
        })
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, request: &FetchRequest) -> CoreResult<String> {
        if let Some(executable) = self.curl_executable.as_deref() {
            return Self::fetch_with_curl(request, executable);
        }

        self.fetch_with_reqwest(request)
    }
}

impl HttpFetcher {
    fn fetch_with_reqwest(&self, request: &FetchRequest) -> CoreResult<String> {
        let mut builder = self.client.get(request.url.clone());

        if !request.headers.is_empty() {
            builder = builder.headers(request.headers.clone());
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

    fn fetch_with_curl(request: &FetchRequest, executable: &Path) -> CoreResult<String> {
        let args = curl_command_args(request)?;
        let output = Command::new(executable)
            .args(args)
            .output()
            .map_err(|source| Error::CurlRequest {
                url: request.url.to_string(),
                executable: executable.display().to_string(),
                source,
            })?;

        if !output.status.success() {
            let details = String::from_utf8_lossy(&output.stderr);
            let message = if details.trim().is_empty() {
                format!("process exited with {}", output.status)
            } else {
                format!("process exited with {}: {}", output.status, details.trim())
            };
            return Err(Error::CurlRequest {
                url: request.url.to_string(),
                executable: executable.display().to_string(),
                source: io::Error::other(message),
            }
            .into());
        }

        let (status, body) =
            parse_curl_response(&output.stdout, &output.stderr).map_err(|source| {
                Error::CurlRequest {
                    url: request.url.to_string(),
                    executable: executable.display().to_string(),
                    source,
                }
            })?;
        if !(200..=299).contains(&status) {
            return Err(http_status_error(&request.url, status, &body).into());
        }

        Ok(body)
    }
}

fn select_curl_executable() -> Option<PathBuf> {
    env::var_os("PATH")
        .as_deref()
        .and_then(|path| select_curl_executable_with(path, is_executable_file))
}

fn select_curl_executable_with<F>(path: &OsStr, is_available: F) -> Option<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    CURL_EXECUTABLES.iter().find_map(|name| {
        env::split_paths(path)
            .map(|directory| directory.join(name))
            .find(|candidate| is_available(candidate))
    })
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn curl_command_args(request: &FetchRequest) -> CoreResult<Vec<OsString>> {
    let anidb_request = is_anidb_url(&request.url);
    let mut args = vec![OsString::from("-s")];

    if anidb_request {
        args.extend([
            OsString::from("-A"),
            OsString::from(anidb::USER_AGENT_VALUE),
        ]);
    }

    args.extend([OsString::from("--max-time"), OsString::from("10")]);

    #[cfg(target_os = "macos")]
    {
        args.extend([
            OsString::from("--ciphers"),
            OsString::from(CURL_CIPHERS),
            OsString::from("--tls13-ciphers"),
            OsString::from(CURL_TLS13_CIPHERS),
        ]);
    }

    if anidb_request {
        for (name, value) in &request.headers {
            let value = value.to_str().map_err(|_| Error::CurlHeader {
                url: request.url.to_string(),
                name: name.to_string(),
            })?;
            args.push(OsString::from("-H"));
            args.push(OsString::from(format!("{name}: {value}")));
        }
    }

    args.push(OsString::from("--write-out"));
    args.push(OsString::from(CURL_STATUS_FORMAT));
    args.push(OsString::from(request.url.to_string()));
    Ok(args)
}

fn parse_curl_response(stdout: &[u8], stderr: &[u8]) -> io::Result<(u16, String)> {
    let status = std::str::from_utf8(stderr)
        .map_err(io::Error::other)?
        .trim()
        .parse::<u16>()
        .map_err(io::Error::other)?;
    if !(100..=599).contains(&status) {
        return Err(io::Error::other(format!("invalid HTTP status {status}")));
    }

    let body = String::from_utf8(stdout.to_vec()).map_err(io::Error::other)?;
    Ok((status, body))
}

pub(crate) fn http_status_error(url: &Url, status: u16, body: &str) -> Error {
    if !(200..=299).contains(&status) && is_anidb_url(url) && body.contains("Just a moment") {
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

/// Validates a URL before it is used as a remote media resource.
///
/// # Errors
///
/// Returns [`Error::InvalidMediaUrl`] unless the URL is HTTP(S), has a host,
/// and contains no username or password.
pub fn validate_media_url(url: &Url) -> CoreResult<()> {
    if matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
    {
        return Ok(());
    }

    Err(Error::InvalidMediaUrl {
        url: url.to_string(),
    }
    .into())
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
    fetcher: F,
    mode: StreamingMode,
    quality: QualityPreference,
}

impl AnimeClient<FixtureFetcher> {
    /// Builds an [`AnimeClient`] backed by fixture data for offline testing.
    ///
    /// # Errors
    ///
    /// Returns an error if fixture responses cannot be loaded.
    pub fn with_fixtures() -> CoreResult<Self> {
        let fetcher = FixtureFetcher::new()?;
        Ok(Self::new(fetcher))
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

        let fetcher = if use_fixtures {
            FetchBackend::fixtures()?
        } else {
            FetchBackend::http()?
        };
        Ok(Self {
            fetcher,
            mode,
            quality,
        })
    }
}

impl<F: Fetcher> AnimeClient<F> {
    pub fn new(fetcher: F) -> Self {
        Self {
            fetcher,
            mode: StreamingMode::Sub,
            quality: QualityPreference::Best,
        }
    }

    /// Fetches anime entries that match the provided `query`.
    ///
    /// # Errors
    ///
    /// Returns an error if the search URL cannot be constructed, the fetcher fails to retrieve
    /// the response body, or the payload cannot be parsed into [`AnimeEntry`] values.
    pub fn search(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
        info!("searching query '{query}' via {}", anidb::ANIDB_SOURCE_ID);

        let entries = self.search_anidb(query)?;

        Self::log_result_count("search", query, entries.len());
        Ok(entries)
    }

    /// Lists the episodes corresponding to the provided `anime_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the episodes URL cannot be constructed, the fetcher fails to retrieve
    /// the response body, or the payload cannot be parsed into [`Episode`] values.
    pub fn list_episodes(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        info!(
            "listing episodes for '{anime_id}' via {}",
            anidb::ANIDB_SOURCE_ID
        );

        let episodes = self.list_episodes_anidb(anime_id)?;

        Self::log_result_count("episode listing", anime_id, episodes.len());
        Ok(episodes)
    }

    /// Resolves a direct stream link for the episode identified by `episode_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream URL cannot be constructed, the fetcher fails to retrieve
    /// JSON, the payload cannot be parsed, or the returned URL is invalid.
    pub fn resolve_stream_url(&self, episode_id: &str) -> CoreResult<StreamLink> {
        info!(
            "resolving stream for '{episode_id}' via {}",
            anidb::ANIDB_SOURCE_ID
        );

        let link = self.resolve_stream_url_anidb(episode_id)?;

        debug!("resolved stream for '{episode_id}' to {}", link.url);
        Ok(link)
    }

    fn resolve_stream_url_anidb(&self, episode_id: &str) -> CoreResult<StreamLink> {
        anidb::validate_episode_id(episode_id)?;
        let languages_url = anidb::languages_url(episode_id)?;
        let languages = self.fetcher.fetch(&Self::get_request(languages_url))?;
        let embed_url = anidb::parse_languages(&languages, self.mode)?;
        let embed = self.fetcher.fetch(&Self::get_request(embed_url))?;
        let master_url = anidb::extract_master_url(&embed)?;
        let master = self.fetcher.fetch(&Self::get_request(master_url.clone()))?;
        let variants = anidb::parse_master_playlist(&master, &master_url)?;
        let selected = anidb::select_variant(&variants, self.quality)?;
        Ok(StreamLink {
            url: selected.url.clone(),
            episode_id: episode_id.to_string(),
            source_id: anidb::ANIDB_SOURCE_ID.to_string(),
        })
    }

    fn search_anidb(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
        let url = anidb::search_url(query)?;
        let body = self.fetcher.fetch(&Self::get_request(url))?;
        anidb::parse_search(&body)
    }

    fn list_episodes_anidb(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        let numeric_id = anidb::anime_numeric_id(anime_id)?;
        let url = anidb::episodes_url(numeric_id)?;
        let body = self.fetcher.fetch(&Self::get_request(url))?;
        anidb::parse_episodes(&body, anime_id)
    }

    fn log_result_count(action: &str, subject: &str, count: usize) {
        if count == 0 {
            warn!("{action} returned no results for '{subject}'");
        } else {
            debug!("{action} returned {count} results for '{subject}'");
        }
    }

    fn get_request(url: Url) -> FetchRequest {
        let request = FetchRequest::get(url);
        if is_anidb_url(&request.url) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::ORIGIN;
    use std::cell::{Cell, RefCell};
    #[cfg(unix)]
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::time::Duration;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[cfg(unix)]
    struct TemporaryExecutable {
        path: PathBuf,
    }

    #[cfg(unix)]
    impl TemporaryExecutable {
        fn new(script: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "animestan-test-curl-{}-{unique}",
                std::process::id()
            ));
            fs::write(&path, script).expect("write temporary curl executable");
            let mut permissions = fs::metadata(&path)
                .expect("temporary curl executable metadata")
                .permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&path, permissions).expect("make curl executable executable");
            Self { path }
        }
    }

    #[cfg(unix)]
    impl Drop for TemporaryExecutable {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[cfg(unix)]
    fn fake_curl(body: &str, status: u16) -> TemporaryExecutable {
        TemporaryExecutable::new(&format!(
            "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"-L\" ] || [ \"$arg\" = \"-sL\" ]; then\n    printf '%s' 'redirect following is forbidden' >&2\n    exit 42\n  fi\ndone\nprintf '%s' '{body}'\nprintf '%s' '{status}' >&2\n"
        ))
    }

    #[cfg(unix)]
    fn failing_fake_curl() -> TemporaryExecutable {
        TemporaryExecutable::new("#!/bin/sh\nprintf '%s' 'fake curl failure' >&2\nexit 7\n")
    }

    #[cfg(unix)]
    fn http_fetcher(executable: &Path) -> HttpFetcher {
        HttpFetcher {
            client: BlockingHttpClient::builder().build().expect("http client"),
            curl_executable: Some(executable.to_path_buf()),
        }
    }

    #[test]
    fn search_returns_results() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let results = client.search("naruto").expect("search results");
        assert!(results.iter().any(|entry| entry.id == "naruto-3686"));
    }

    #[test]
    fn fixture_key_retains_get_prefix() {
        let request = FetchRequest::get(Url::parse("https://anidb.app/browse?q=naruto").unwrap());
        assert_eq!(request.key(), "GET|https://anidb.app/browse?q=naruto");
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
        let client = AnimeClient::new(CapturingFetcher {
            headers: Rc::clone(&headers),
        });

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
    fn curl_executable_selection_prefers_browser_impersonator() {
        let path = env::join_paths([Path::new("/first"), Path::new("/second")]).unwrap();
        let selected = super::select_curl_executable_with(&path, |candidate| {
            candidate == Path::new("/second/curl_firefox135")
                || candidate == Path::new("/first/curl")
        });

        assert_eq!(selected, Some(PathBuf::from("/second/curl_firefox135")));
    }

    #[test]
    fn curl_command_args_preserve_anidb_headers() {
        let request = FetchRequest::get(Url::parse("https://anidb.app/browse?q=naruto").unwrap())
            .with_header(
                USER_AGENT,
                HeaderValue::from_static(anidb::USER_AGENT_VALUE),
            )
            .with_header(REFERER, HeaderValue::from_static(anidb::BASE_URL));
        let args = super::curl_command_args(&request)
            .expect("curl arguments")
            .into_iter()
            .map(|arg| arg.into_string().expect("utf-8 curl argument"))
            .collect::<Vec<_>>();
        let user_agent_header = format!("user-agent: {}", anidb::USER_AGENT_VALUE);
        let referer_header = format!("referer: {}", anidb::BASE_URL);

        assert_eq!(
            args[0..5],
            ["-s", "-A", anidb::USER_AGENT_VALUE, "--max-time", "10"]
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-H".to_string(), user_agent_header.clone()])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-H".to_string(), referer_header.clone()])
        );
        assert_eq!(args.last(), Some(&request.url.to_string()));

        #[cfg(target_os = "macos")]
        {
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--ciphers".to_string(), super::CURL_CIPHERS.to_string()])
            );
            assert!(args.windows(2).any(|pair| pair
                == [
                    "--tls13-ciphers".to_string(),
                    super::CURL_TLS13_CIPHERS.to_string()
                ]));
        }
    }

    #[test]
    fn curl_command_args_omit_anidb_headers_for_external_urls() {
        let request = FetchRequest::get(Url::parse("https://stream.example/master.m3u8").unwrap())
            .with_header(
                USER_AGENT,
                HeaderValue::from_static(anidb::USER_AGENT_VALUE),
            )
            .with_header(REFERER, HeaderValue::from_static(anidb::BASE_URL));
        let args = super::curl_command_args(&request).expect("curl arguments");

        assert!(!args.iter().any(|arg| arg == "-H"));
        assert!(!args.iter().any(|arg| arg == "-A"));
        assert!(!args.iter().any(|arg| arg == anidb::USER_AGENT_VALUE));
    }

    #[cfg(unix)]
    #[test]
    fn curl_transport_keeps_anidb_search_working_without_following_redirects() {
        let fake = fake_curl(r#"<a href="/anime/naruto-3686" title="Naruto"></a>"#, 200);
        let client = AnimeClient::new(http_fetcher(&fake.path));

        let results = client.search("naruto").expect("curl search results");

        assert!(results.iter().any(|entry| entry.id == "naruto-3686"));
    }

    #[cfg(unix)]
    #[test]
    fn curl_transport_returns_redirect_status_without_following_it() {
        let fake = fake_curl("redirect response", 302);
        let fetcher = http_fetcher(&fake.path);
        let request = FetchRequest::get(Url::parse("https://anidb.app/browse?q=naruto").unwrap());

        let error = fetcher.fetch(&request).expect_err("redirect response");

        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::HttpStatus { status: 302, .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn curl_transport_reports_selected_process_failure() {
        let fake = failing_fake_curl();
        let fetcher = http_fetcher(&fake.path);
        let request = FetchRequest::get(Url::parse("https://anidb.app/browse?q=naruto").unwrap());

        let error = fetcher.fetch(&request).expect_err("curl process failure");

        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::CurlRequest {
                url,
                executable,
                source,
            }) if url == request.url.as_str()
                && executable == fake.path.to_string_lossy().as_ref()
                && source.to_string().contains("fake curl failure")
        ));
    }

    #[test]
    fn curl_response_status_is_separate_from_body() {
        let (status, body) =
            super::parse_curl_response(b"<title>Just a moment...</title>\n200\n", b"403")
                .expect("curl response");
        let url = Url::parse("https://anidb.app/browse?q=naruto").unwrap();

        assert_eq!(status, 403);
        assert_eq!(body, "<title>Just a moment...</title>\n200\n".to_string());
        assert!(matches!(
            super::http_status_error(&url, status, &body),
            Error::ProviderBlocked { .. }
        ));
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
        let error = super::http_status_error(&url, 500, "<title>internal server error</title>");

        assert!(
            matches!(error, Error::HttpStatus { url: error_url, status } if error_url == url.to_string() && status == 500)
        );
    }

    #[test]
    fn challenge_body_on_non_403_status_reports_provider_blocked() {
        let url = Url::parse("https://anidb.app/browse?q=naruto").unwrap();
        let error = super::http_status_error(&url, 500, "<title>Just a moment...</title>");

        assert!(matches!(error, Error::ProviderBlocked { .. }));
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
        let client = AnimeClient::new(CountingFetcher {
            calls: Rc::clone(&calls),
        });

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
        let request = AnimeClient::<CapturingFetcher>::get_request(
            Url::parse("https://stream.example/master.m3u8").unwrap(),
        );

        assert!(request.headers.get(USER_AGENT).is_none());
        assert!(request.headers.get(REFERER).is_none());
        assert!(request.headers.get(ORIGIN).is_none());
    }
}
