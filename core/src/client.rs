// Copyright (C) 2026 M.R. Siavash Katebzadeg <mr@katebzadeh.xyz>
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

use crate::error::Error;
use crate::fixtures;
use crate::models::{AnimeEntry, Episode, StreamLink};
use crate::source::SourceDefinition;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use url::Url;

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
        let url = self.source.search.render(&[("query", query)])?;
        let mut entries: Vec<AnimeEntry> = self.fetch_and_parse(&url)?;
        for entry in &mut entries {
            entry.source_id.clone_from(&self.source.id);
        }
        Ok(entries)
    }

    /// Lists the episodes corresponding to the provided `anime_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the episodes URL cannot be rendered, the fetcher fails to retrieve
    /// JSON, or the payload cannot be parsed into [`Episode`] values.
    pub fn list_episodes(&self, anime_id: &str) -> Result<Vec<Episode>, Error> {
        let url = self.source.episodes.render(&[("anime_id", anime_id)])?;
        let mut episodes: Vec<Episode> = self.fetch_and_parse(&url)?;
        for episode in &mut episodes {
            episode.source_id.clone_from(&self.source.id);
        }
        Ok(episodes)
    }

    /// Resolves a direct stream link for the episode identified by `episode_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream URL cannot be rendered, the fetcher fails to retrieve
    /// JSON, the payload cannot be parsed, or the returned URL is invalid.
    pub fn resolve_stream_url(&self, episode_id: &str) -> Result<StreamLink, Error> {
        let url = self.source.stream.render(&[("episode_id", episode_id)])?;
        let payload: StreamPayload = self.fetch_and_parse(&url)?;
        let stream_url = Url::parse(&payload.url).map_err(|source| Error::StreamUrlParse {
            url: payload.url.clone(),
            source,
        })?;

        Ok(StreamLink {
            url: stream_url,
            episode_id: episode_id.to_owned(),
            source_id: self.source.id.clone(),
        })
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
