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

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde::Deserialize;
use serde_json::json;
use url::{Url, form_urlencoded::byte_serialize};

use crate::{error::Error, source::ALLANIME_API_ENDPOINT};

use super::{
    AllMangaMetadataProvider, AnimeMetadata, MetadataCache, MetadataProvider, MetadataSource,
    normalize_query,
};

const ALLMANGA_REFERER: &str = "https://allmanga.to";
const ALLMANGA_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0";

const ALLMANGA_SEARCH_QUERY: &str = "query ($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType ) { shows( search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin ) { edges { _id name } }}";
const ALLMANGA_DETAILS_QUERY: &str = "query ($showId: String!) { show(_id: $showId) { _id name description genres studios status score } }";
const ALLMANGA_SEASON_QUERY: &str =
    "query ($showId: String!) { show(_id: $showId) { _id season { quarter year } } }";

impl MetadataProvider for AllMangaMetadataProvider {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, Error> {
        let key = normalize_query(query);
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_allmanga(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }
}

impl AllMangaMetadataProvider {
    #[must_use]
    pub fn new(client: Client) -> Self {
        let client = enrich_client(client);
        Self {
            client,
            cache: MetadataCache::default(),
        }
    }

    fn fetch_allmanga(&self, query: &str) -> Result<AnimeMetadata, Error> {
        let search_id = self.search_show_id(query)?;
        let mut show = self.fetch_show_details(&search_id)?;
        if let Ok(season) = self.fetch_show_season(&search_id) {
            show.season = season;
        }
        Ok(show_to_metadata(show, query))
    }

    fn search_show_id(&self, query: &str) -> Result<String, Error> {
        let variables = json!({
            "search": {
                "allowAdult": false,
                "allowUnknown": false,
                "query": query,
            },
            "limit": 1,
            "page": 1,
            "translationType": "sub",
            "countryOrigin": "ALL",
        });
        let url = build_graphql_url(ALLMANGA_SEARCH_QUERY, &variables)?;
        let response =
            self.client
                .get(url.clone())
                .send()
                .map_err(|source| Error::HttpRequest {
                    url: url.to_string(),
                    source,
                })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: url.to_string(),
            source,
        })?;
        let payload = serde_json::from_str::<AllMangaSearchResponse>(&body).map_err(|source| {
            Error::ResponseParse {
                url: url.to_string(),
                source,
            }
        })?;
        let entry =
            payload
                .data
                .shows
                .edges
                .into_iter()
                .next()
                .ok_or_else(|| Error::MetadataNotFound {
                    query: query.to_string(),
                })?;
        Ok(entry.id)
    }

    fn fetch_show_details(&self, show_id: &str) -> Result<AllMangaShow, Error> {
        let variables = json!({ "showId": show_id });
        let url = build_graphql_url(ALLMANGA_DETAILS_QUERY, &variables)?;
        let response =
            self.client
                .get(url.clone())
                .send()
                .map_err(|source| Error::HttpRequest {
                    url: url.to_string(),
                    source,
                })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: url.to_string(),
            source,
        })?;
        let payload = serde_json::from_str::<AllMangaDetailsResponse>(&body).map_err(|source| {
            Error::ResponseParse {
                url: url.to_string(),
                source,
            }
        })?;
        payload.data.show.ok_or_else(|| Error::MetadataNotFound {
            query: show_id.to_string(),
        })
    }

    fn fetch_show_season(&self, show_id: &str) -> Result<Option<AllMangaSeason>, Error> {
        let variables = json!({ "showId": show_id });
        let url = build_graphql_url(ALLMANGA_SEASON_QUERY, &variables)?;
        let response =
            self.client
                .get(url.clone())
                .send()
                .map_err(|source| Error::HttpRequest {
                    url: url.to_string(),
                    source,
                })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let body = response.text().map_err(|source| Error::HttpBodyParse {
            url: url.to_string(),
            source,
        })?;
        let payload = serde_json::from_str::<AllMangaSeasonResponse>(&body).map_err(|source| {
            Error::ResponseParse {
                url: url.to_string(),
                source,
            }
        })?;
        Ok(payload.data.show.and_then(|show| show.season))
    }
}

fn enrich_client(client: Client) -> Client {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(ALLMANGA_USER_AGENT));
    headers.insert(REFERER, HeaderValue::from_static(ALLMANGA_REFERER));
    Client::builder()
        .default_headers(headers)
        .build()
        .unwrap_or(client)
}

fn build_graphql_url(query: &str, variables: &serde_json::Value) -> Result<Url, Error> {
    let mut url = Url::parse(ALLANIME_API_ENDPOINT).map_err(|source| Error::InvalidUrl {
        template: ALLANIME_API_ENDPOINT.to_string(),
        source,
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("variables", &variables.to_string());
        pairs.append_pair("query", query);
    }
    Ok(url)
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaSearchResponse {
    #[serde(default)]
    data: AllMangaSearchData,
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaSearchData {
    #[serde(default)]
    shows: AllMangaShows,
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaShows {
    #[serde(default)]
    edges: Vec<AllMangaShowEdge>,
}

#[derive(Debug, Deserialize)]
struct AllMangaShowEdge {
    #[serde(rename = "_id")]
    id: String,
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaDetailsResponse {
    #[serde(default)]
    data: AllMangaShowData,
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaShowData {
    show: Option<AllMangaShow>,
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaSeasonResponse {
    #[serde(default)]
    data: AllMangaSeasonData,
}

#[derive(Debug, Deserialize, Default)]
struct AllMangaSeasonData {
    show: Option<AllMangaSeasonShow>,
}

#[derive(Debug, Deserialize)]
struct AllMangaSeasonShow {
    season: Option<AllMangaSeason>,
}

#[derive(Debug, Deserialize)]
struct AllMangaShow {
    name: String,
    description: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    studios: Vec<String>,
    status: Option<String>,
    score: Option<f32>,
    season: Option<AllMangaSeason>,
}

#[derive(Debug, Deserialize)]
struct AllMangaSeason {
    quarter: Option<String>,
    year: Option<u16>,
}

fn show_to_metadata(show: AllMangaShow, query: &str) -> AnimeMetadata {
    let season = show.season.as_ref().and_then(|s| s.quarter.clone());
    let year = show.season.as_ref().and_then(|s| s.year);
    AnimeMetadata {
        title: show.name.clone(),
        synopsis: show.description,
        score: show.score,
        genres: show.genres,
        studios: show.studios,
        status: show.status,
        season,
        year,
        trailer_url: None,
        source_url: source_url(&show.name, query),
        source: MetadataSource::AllManga,
    }
}

fn source_url(title: &str, query: &str) -> String {
    let search = if title.trim().is_empty() {
        query
    } else {
        title
    };
    let encoded: String = byte_serialize(search.as_bytes()).collect();
    format!("https://allmanga.to/anime?search={encoded}")
}

#[cfg(test)]
mod tests {
    use super::{AllMangaMetadataProvider, MetadataProvider};

    #[test]
    fn allmanga_fetches_live_metadata() {
        if std::env::var("ANIMESTAN_LIVE_METADATA").is_err() {
            return;
        }
        let provider = AllMangaMetadataProvider::default();
        let metadata = provider
            .fetch_by_query("naruto")
            .expect("allmanga metadata");
        assert!(!metadata.title.is_empty());
    }
}
