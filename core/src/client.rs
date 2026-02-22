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
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use spdlog::prelude::*;
use std::collections::HashMap;
use std::env;
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

use crate::client::allanime::{
    ALLANIME_EPISODE_EMBED_GQL, ALLANIME_EPISODES_GQL, ALLANIME_REFERER, ALLANIME_SEARCH_GQL,
    ALLANIME_TRANSLATION, ALLANIME_USER_AGENT, AllAnimeEpisodeEmbedResponse,
    AllAnimeEpisodesResponse, AllAnimeSearchResponse, build_embed_url, build_graphql_url,
    decode_source_url, parse_episode_number, select_source_url, select_stream_url,
    split_episode_id,
};

const FIXTURES_ENV: &str = "ANIMESTAN_USE_FIXTURES";

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

pub trait Fetcher {
    /// Fetches the JSON payload located at `url`.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the request cannot be fulfilled or the response
    /// body cannot be converted into JSON.
    fn fetch_json(&self, url: &Url) -> CoreResult<Value>;
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
    pub fn new() -> CoreResult<Self> {
        let responses = fixtures::load_responses()?;
        Ok(Self { responses })
    }
}

impl Fetcher for FixtureFetcher {
    fn fetch_json(&self, url: &Url) -> CoreResult<Value> {
        let value =
            self.responses
                .get(url.as_str())
                .cloned()
                .ok_or_else(|| Error::MissingFixture {
                    url: url.to_string(),
                })?;
        Ok(value)
    }
}

pub struct HttpFetcher {
    client: BlockingHttpClient,
}

impl HttpFetcher {
    fn new() -> CoreResult<Self> {
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
    fn fetch_json(&self, url: &Url) -> CoreResult<Value> {
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
            }
            .into());
        }

        let value = response
            .json::<Value>()
            .map_err(|source| Error::HttpBodyParse {
                url: url.to_string(),
                source,
            })?;
        Ok(value)
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
    fn fetch_json(&self, url: &Url) -> CoreResult<Value> {
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
            let source = SourceDefinition::allanime();
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
    /// JSON, or the payload cannot be parsed into [`AnimeEntry`] values.
    pub fn search(&self, query: &str) -> CoreResult<Vec<AnimeEntry>> {
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
    pub fn list_episodes(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
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
    pub fn resolve_stream_url(&self, episode_id: &str) -> CoreResult<StreamLink> {
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
        let url = build_graphql_url(ALLANIME_SEARCH_GQL, &variables);
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

    fn list_episodes_allanime(&self, anime_id: &str) -> CoreResult<Vec<Episode>> {
        let variables = json!({ "showId": anime_id });
        let url = build_graphql_url(ALLANIME_EPISODES_GQL, &variables);
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
                synopsis: None,
                duration_secs: None,
                air_date: None,
            })
            .collect();

        episodes.sort_by(|a, b| a.number.cmp(&b.number));
        Ok(episodes)
    }

    fn resolve_stream_url_allanime(&self, episode_id: &str) -> CoreResult<StreamLink> {
        let (show_id, episode_string) = split_episode_id(episode_id)?;

        let variables = json!({
            "showId": &show_id,
            "translationType": ALLANIME_TRANSLATION,
            "episodeString": &episode_string,
        });
        let url = build_graphql_url(ALLANIME_EPISODE_EMBED_GQL, &variables);
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

    fn fetch_and_parse<T>(&self, url: &Url) -> CoreResult<T>
    where
        T: DeserializeOwned,
    {
        let value = self.fetcher.fetch_json(url)?;
        let parsed = serde_json::from_value(value).map_err(|source| Error::ResponseParse {
            url: url.to_string(),
            source,
        })?;
        Ok(parsed)
    }

    fn uses_allanime(&self) -> bool {
        self.source.id == SourceDefinition::ALLANIME_ID
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
