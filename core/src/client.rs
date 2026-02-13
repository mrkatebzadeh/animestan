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

use std::collections::HashMap;
use std::env;
use std::str;

use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use spdlog::prelude::*;
use url::Url;

use crate::config::AppConfig;
use crate::error::Error;
use crate::fixtures;
use crate::models::{AnimeEntry, Episode, StreamLink};
use crate::source::{SourceDefinition, ALLANIME_API_ENDPOINT};

const FIXTURES_ENV: &str = "ANIMESTAN_USE_FIXTURES";
const ALLANIME_TRANSLATION: &str = "sub";
const ALLANIME_REFERER: &str = "https://allmanga.to";
const ALLANIME_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0";
const ALLANIME_EMBED_HOST: &str = "https://allanime.day";
const ALLANIME_SEARCH_GQL: &str = "query ($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType ) { shows( search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin ) { edges { _id name availableEpisodes __typename } }}";
const ALLANIME_EPISODES_GQL: &str =
    "query ($showId: String!) { show( _id: $showId ) { _id availableEpisodesDetail }}";
const ALLANIME_EPISODE_EMBED_GQL: &str = "query ($showId: String!, $translationType: VaildTranslationTypeEnumType!, $episodeString: String!) { episode( showId: $showId translationType: $translationType episodeString: $episodeString ) { episodeString sourceUrls }}";

pub trait Fetcher {
    /// Fetches the JSON payload located at `url`.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the request cannot be fulfilled or the response
    /// body cannot be converted into JSON.
    fn fetch_json(&self, url: &Url) -> Result<Value, Error>;
}

pub struct FixtureFetcher {
    responses: HashMap<String, Value>,
}

impl FixtureFetcher {
    /// Creates a [`FixtureFetcher`] preloaded with static responses.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the fixture response data cannot be loaded
    /// from disk.
    pub fn new() -> Result<Self, Error> {
        let responses = fixtures::load_responses()?;
        Ok(Self { responses })
    }
}

impl Fetcher for FixtureFetcher {
    fn fetch_json(&self, url: &Url) -> Result<Value, Error> {
        self.responses
            .get(url.as_str())
            .cloned()
            .ok_or_else(|| Error::MissingFixture {
                url: url.to_string(),
            })
    }
}

pub struct HttpFetcher {
    client: BlockingHttpClient,
}

impl HttpFetcher {
    fn new() -> Result<Self, Error> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(ALLANIME_USER_AGENT));
        headers.insert(REFERER, HeaderValue::from_static(ALLANIME_REFERER));

        let client = BlockingHttpClient::builder()
            .default_headers(headers)
            .build()
            .map_err(Error::HttpClient)?;

        Ok(Self { client })
    }
}

impl Fetcher for HttpFetcher {
    fn fetch_json(&self, url: &Url) -> Result<Value, Error> {
        let response =
            self.client
                .get(url.clone())
                .send()
                .map_err(|source| Error::HttpRequest {
                    url: url.to_string(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: response.status().as_u16(),
            });
        }

        response
            .json::<Value>()
            .map_err(|source| Error::HttpBodyParse {
                url: url.to_string(),
                source,
            })
    }
}

pub enum FetchBackend {
    Fixtures(FixtureFetcher),
    Http(HttpFetcher),
}

impl FetchBackend {
    fn fixtures() -> Result<Self, Error> {
        Ok(Self::Fixtures(FixtureFetcher::new()?))
    }

    fn http() -> Result<Self, Error> {
        Ok(Self::Http(HttpFetcher::new()?))
    }
}

impl Fetcher for FetchBackend {
    fn fetch_json(&self, url: &Url) -> Result<Value, Error> {
        match self {
            Self::Fixtures(fetcher) => fetcher.fetch_json(url),
            Self::Http(fetcher) => fetcher.fetch_json(url),
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
    pub fn with_fixtures() -> Result<Self, Error> {
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
    pub fn with_env() -> Result<Self, Error> {
        let config = AppConfig::load_default()?;
        Self::from_config(&config)
    }

    /// Builds an [`AnimeClient`] from the provided configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if fixture data must be loaded but is unavailable or malformed, or the
    /// HTTP client cannot be constructed.
    pub fn from_config(config: &AppConfig) -> Result<Self, Error> {
        let use_fixtures = config.use_fixtures.unwrap_or_else(fixtures_fetch_enabled);

        if use_fixtures {
            let source = Self::select_fixture_source(config)?;
            let fetcher = FetchBackend::fixtures()?;
            Ok(Self { source, fetcher })
        } else {
            let source = SourceDefinition::allanime();
            let fetcher = FetchBackend::http()?;
            Ok(Self { source, fetcher })
        }
    }

    fn select_fixture_source(config: &AppConfig) -> Result<SourceDefinition, Error> {
        let catalog = fixtures::load_catalog()?;
        if let Some(source_id) = config.source_id.as_deref() {
            catalog
                .source_by_id(source_id)
                .ok_or_else(|| Error::UnknownSourceId {
                    source_id: source_id.to_string(),
                })
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
    /// JSON, or the payload cannot be parsed into [`AnimeEntry`] values.
    pub fn search(&self, query: &str) -> Result<Vec<AnimeEntry>, Error> {
        info!("searching query '{query}' via {}", self.source.id);

        let entries = if self.uses_allanime() {
            self.search_allanime(query)?
        } else {
            let url = self.source.search.render(&[("query", query)])?;
            let mut entries: Vec<AnimeEntry> = self.fetch_and_parse(&url)?;
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
    /// JSON, or the payload cannot be parsed into [`Episode`] values.
    pub fn list_episodes(&self, anime_id: &str) -> Result<Vec<Episode>, Error> {
        info!("listing episodes for '{anime_id}' via {}", self.source.id);

        let episodes = if self.uses_allanime() {
            self.list_episodes_allanime(anime_id)?
        } else {
            let url = self.source.episodes.render(&[("anime_id", anime_id)])?;
            let mut episodes: Vec<Episode> = self.fetch_and_parse(&url)?;
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
    pub fn resolve_stream_url(&self, episode_id: &str) -> Result<StreamLink, Error> {
        info!("resolving stream for '{episode_id}' via {}", self.source.id);

        let link = if self.uses_allanime() {
            self.resolve_stream_url_allanime(episode_id)?
        } else {
            let url = self.source.stream.render(&[("episode_id", episode_id)])?;
            let payload: StreamPayload = self.fetch_and_parse(&url)?;
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

    fn search_allanime(&self, query: &str) -> Result<Vec<AnimeEntry>, Error> {
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
        let url = build_allanime_graphql_url(ALLANIME_SEARCH_GQL, &variables);
        let payload: AllAnimeSearchResponse = self.fetch_and_parse(&url)?;

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

    fn list_episodes_allanime(&self, anime_id: &str) -> Result<Vec<Episode>, Error> {
        let variables = json!({ "showId": anime_id });
        let url = build_allanime_graphql_url(ALLANIME_EPISODES_GQL, &variables);
        let payload: AllAnimeEpisodesResponse = self.fetch_and_parse(&url)?;

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
            })
            .collect();

        episodes.sort_by(|a, b| a.number.cmp(&b.number));
        Ok(episodes)
    }

    fn resolve_stream_url_allanime(&self, episode_id: &str) -> Result<StreamLink, Error> {
        let (show_id, episode_string) = split_episode_id(episode_id)?;

        let variables = json!({
            "showId": &show_id,
            "translationType": ALLANIME_TRANSLATION,
            "episodeString": &episode_string,
        });
        let url = build_allanime_graphql_url(ALLANIME_EPISODE_EMBED_GQL, &variables);
        let payload: AllAnimeEpisodeEmbedResponse = self.fetch_and_parse(&url)?;

        let episode = payload
            .data
            .episode
            .ok_or_else(|| Error::StreamResolution {
                message: "episode metadata missing".to_string(),
            })?;

        let source =
            select_source_url(&episode.source_urls).ok_or_else(|| Error::StreamResolution {
                message: "no source URLs returned".to_string(),
            })?;

        let decoded = decode_source_url(&source.source_url)?;
        let embed_url = build_allanime_embed_url(&decoded)?;
        let payload = self.fetcher.fetch_json(&embed_url)?;
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

    fn fetch_and_parse<T>(&self, url: &Url) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let value = self.fetcher.fetch_json(url)?;
        serde_json::from_value(value).map_err(|source| Error::ResponseParse {
            url: url.to_string(),
            source,
        })
    }

    fn uses_allanime(&self) -> bool {
        self.source.id == SourceDefinition::ALLANIME_ID
    }
}

#[derive(Debug, Deserialize)]
struct StreamPayload {
    url: String,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeSearchResponse {
    #[serde(default)]
    data: AllAnimeSearchData,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeSearchData {
    #[serde(default)]
    shows: AllAnimeShows,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeShows {
    #[serde(default)]
    edges: Vec<AllAnimeShowEdge>,
}

#[derive(Debug, Deserialize)]
struct AllAnimeShowEdge {
    #[serde(rename = "_id")]
    id: String,
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeEpisodesResponse {
    #[serde(default)]
    data: AllAnimeEpisodesData,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeEpisodesData {
    show: Option<AllAnimeShowDetail>,
}

#[derive(Debug, Deserialize)]
struct AllAnimeShowDetail {
    #[serde(rename = "_id")]
    id: String,
    #[serde(rename = "availableEpisodesDetail", default)]
    available_episodes_detail: AllAnimeEpisodeDetail,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeEpisodeDetail {
    #[serde(default)]
    sub: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeEpisodeEmbedResponse {
    #[serde(default)]
    data: AllAnimeEpisodeData,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeEpisodeData {
    episode: Option<AllAnimeEpisodeInfo>,
}

#[derive(Debug, Deserialize, Default)]
struct AllAnimeEpisodeInfo {
    #[serde(rename = "sourceUrls", default)]
    source_urls: Vec<AllAnimeSourceUrl>,
}

#[derive(Debug, Deserialize, Clone)]
struct AllAnimeSourceUrl {
    #[serde(rename = "sourceUrl")]
    source_url: String,
    #[serde(rename = "sourceName")]
    source_name: Option<String>,
}

#[derive(Debug, Clone)]
struct StreamCandidate {
    url: String,
    resolution: Option<u32>,
}

fn fixtures_fetch_enabled() -> bool {
    env::var(FIXTURES_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn build_allanime_graphql_url(query: &str, variables: &Value) -> Url {
    let mut url = Url::parse(ALLANIME_API_ENDPOINT).expect("valid AllAnime API endpoint");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("variables", &variables.to_string());
        pairs.append_pair("query", query);
    }
    url
}

fn build_allanime_embed_url(decoded: &str) -> Result<Url, Error> {
    let trimmed = decoded.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Url::parse(trimmed).map_err(|source| Error::InvalidUrl {
            template: trimmed.to_string(),
            source,
        })
    } else {
        let full = format!("{ALLANIME_EMBED_HOST}{trimmed}");
        Url::parse(&full).map_err(|source| Error::InvalidUrl {
            template: full,
            source,
        })
    }
}

fn select_source_url(sources: &[AllAnimeSourceUrl]) -> Option<&AllAnimeSourceUrl> {
    sources
        .iter()
        .find(|source| source.source_name.as_deref() == Some("Default"))
        .or_else(|| sources.first())
}

fn split_episode_id(episode_id: &str) -> Result<(String, String), Error> {
    let mut parts = episode_id.splitn(2, ':');
    let show_id = parts.next();
    let episode = parts.next();

    match (show_id, episode) {
        (Some(show), Some(ep)) if !show.is_empty() && !ep.is_empty() => {
            Ok((show.to_string(), ep.to_string()))
        }
        _ => Err(Error::EpisodeIdParse {
            episode_id: episode_id.to_string(),
        }),
    }
}

fn decode_source_url(encoded: &str) -> Result<String, Error> {
    let stripped = encoded.trim_start_matches('-');
    if stripped.len() % 2 != 0 {
        return Err(Error::StreamResolution {
            message: "invalid encoded source length".to_string(),
        });
    }

    let mut decoded = String::with_capacity(stripped.len() / 2);
    for chunk in stripped.as_bytes().chunks(2) {
        let pair = str::from_utf8(chunk).map_err(|_| Error::StreamResolution {
            message: "encoded source not utf-8".to_string(),
        })?;
        let pair_lower = pair.to_ascii_lowercase();
        let ch = decode_pair(&pair_lower).ok_or_else(|| Error::StreamResolution {
            message: format!("unknown source code {pair}"),
        })?;
        decoded.push(ch);
    }

    Ok(decoded.replace("/clock", "/clock.json"))
}

#[allow(clippy::too_many_lines)]
fn decode_pair(pair: &str) -> Option<char> {
    match pair {
        "79" => Some('A'),
        "7a" => Some('B'),
        "7b" => Some('C'),
        "7c" => Some('D'),
        "7d" => Some('E'),
        "7e" => Some('F'),
        "7f" => Some('G'),
        "70" => Some('H'),
        "71" => Some('I'),
        "72" => Some('J'),
        "73" => Some('K'),
        "74" => Some('L'),
        "75" => Some('M'),
        "76" => Some('N'),
        "77" => Some('O'),
        "68" => Some('P'),
        "69" => Some('Q'),
        "6a" => Some('R'),
        "6b" => Some('S'),
        "6c" => Some('T'),
        "6d" => Some('U'),
        "6e" => Some('V'),
        "6f" => Some('W'),
        "60" => Some('X'),
        "61" => Some('Y'),
        "62" => Some('Z'),
        "59" => Some('a'),
        "5a" => Some('b'),
        "5b" => Some('c'),
        "5c" => Some('d'),
        "5d" => Some('e'),
        "5e" => Some('f'),
        "5f" => Some('g'),
        "50" => Some('h'),
        "51" => Some('i'),
        "52" => Some('j'),
        "53" => Some('k'),
        "54" => Some('l'),
        "55" => Some('m'),
        "56" => Some('n'),
        "57" => Some('o'),
        "48" => Some('p'),
        "49" => Some('q'),
        "4a" => Some('r'),
        "4b" => Some('s'),
        "4c" => Some('t'),
        "4d" => Some('u'),
        "4e" => Some('v'),
        "4f" => Some('w'),
        "40" => Some('x'),
        "41" => Some('y'),
        "42" => Some('z'),
        "08" => Some('0'),
        "09" => Some('1'),
        "0a" => Some('2'),
        "0b" => Some('3'),
        "0c" => Some('4'),
        "0d" => Some('5'),
        "0e" => Some('6'),
        "0f" => Some('7'),
        "00" => Some('8'),
        "01" => Some('9'),
        "15" => Some('-'),
        "16" => Some('.'),
        "67" => Some('_'),
        "46" => Some('~'),
        "02" => Some(':'),
        "17" => Some('/'),
        "07" => Some('?'),
        "1b" => Some('#'),
        "63" => Some('['),
        "65" => Some(']'),
        "78" => Some('@'),
        "19" => Some('!'),
        "1c" => Some('$'),
        "1e" => Some('&'),
        "10" => Some('('),
        "11" => Some(')'),
        "12" => Some('*'),
        "13" => Some('+'),
        "14" => Some(','),
        "03" => Some(';'),
        "05" => Some('='),
        "1d" => Some('%'),
        _ => None,
    }
}

fn select_stream_url(value: &Value) -> Result<String, Error> {
    let mut candidates = Vec::new();
    collect_stream_candidates(value, &mut candidates);
    if candidates.is_empty() {
        return Err(Error::StreamResolution {
            message: "no playable links found".to_string(),
        });
    }

    let mut best = candidates.remove(0);
    for candidate in candidates {
        if prefers_candidate(&candidate, &best) {
            best = candidate;
        }
    }

    Ok(best.url)
}

fn collect_stream_candidates(value: &Value, candidates: &mut Vec<StreamCandidate>) {
    match value {
        Value::Object(map) => {
            if let (Some(link), Some(resolution)) = (map.get("link"), map.get("resolutionStr")) {
                if let (Some(link), Some(resolution)) = (link.as_str(), resolution.as_str()) {
                    candidates.push(StreamCandidate {
                        url: link.to_string(),
                        resolution: parse_resolution(resolution),
                    });
                }
            } else if let Some(url) = map.get("url").and_then(Value::as_str) {
                if map
                    .get("hardsub_lang")
                    .and_then(Value::as_str)
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("en-US"))
                {
                    candidates.push(StreamCandidate {
                        url: url.to_string(),
                        resolution: None,
                    });
                }
            }

            for value in map.values() {
                collect_stream_candidates(value, candidates);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_stream_candidates(item, candidates);
            }
        }
        _ => {}
    }
}

fn prefers_candidate(candidate: &StreamCandidate, current: &StreamCandidate) -> bool {
    match (candidate.resolution, current.resolution) {
        (Some(candidate_res), Some(current_res)) => candidate_res > current_res,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn parse_resolution(text: &str) -> Option<u32> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn parse_episode_number(label: &str) -> u32 {
    let segment = label.split('.').next().unwrap_or(label);
    segment.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_returns_results() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let results = client.search("naruto").expect("search results");
        assert!(results.iter().any(|entry| entry.id == "naruto"));
    }

    #[test]
    fn list_episodes_returns_entries() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let episodes = client.list_episodes("naruto").expect("episode listing");
        assert!(episodes.iter().any(|episode| episode.id == "naruto-1"));
    }

    #[test]
    fn resolve_stream_url_returns_url() {
        let client = AnimeClient::with_fixtures().expect("fixtures client");
        let stream = client
            .resolve_stream_url("naruto-1")
            .expect("stream resolution");
        assert_eq!(stream.url.as_str(), "https://stream.example/naruto-1.m3u8");
    }
}
