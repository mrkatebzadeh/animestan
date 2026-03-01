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
use serde::Deserialize;

use crate::{AppConfig, error::Error as CoreError};

use super::{
    AnimeMetadata, KitsuMetadataProvider, MetadataCache, MetadataProvider, MetadataSource,
    normalize_query,
};

const KITSU_SEARCH_URL: &str = "https://kitsu.io/api/edge/anime";

impl MetadataProvider for KitsuMetadataProvider {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let key = format!("kitsu:{}", normalize_query(query));
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_kitsu(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }
}

impl KitsuMetadataProvider {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self::with_cache(
            client,
            MetadataCache::new(AppConfig::default().metadata_cache_path()),
        )
    }

    pub(super) fn with_cache(client: Client, cache: MetadataCache) -> Self {
        Self { client, cache }
    }

    fn fetch_kitsu(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let response = self
            .client
            .get(KITSU_SEARCH_URL)
            .query(&[("filter[text]", query)])
            .send()
            .map_err(|source| CoreError::HttpRequest {
                url: KITSU_SEARCH_URL.to_string(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::HttpStatus {
                url: KITSU_SEARCH_URL.to_string(),
                status: status.as_u16(),
            });
        }
        let body = response.text().map_err(|source| CoreError::HttpBodyParse {
            url: KITSU_SEARCH_URL.to_string(),
            source,
        })?;
        let data = serde_json::from_str::<KitsuResponse>(&body).map_err(|source| {
            CoreError::ResponseParse {
                url: KITSU_SEARCH_URL.to_string(),
                source,
            }
        })?;
        let record = data
            .data
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::MetadataNotFound {
                query: query.to_string(),
            })?;
        Ok(record_to_metadata(record, query))
    }
}

#[derive(Deserialize)]
struct KitsuResponse {
    data: Vec<KitsuRecord>,
}

#[derive(Deserialize)]
struct KitsuRecord {
    id: String,
    attributes: KitsuAttributes,
}

#[derive(Deserialize)]
struct KitsuAttributes {
    #[serde(rename = "canonicalTitle")]
    canonical_title: Option<String>,
    #[serde(rename = "englishTitle")]
    english_title: Option<String>,
    synopsis: Option<String>,
    #[serde(rename = "averageRating")]
    average_rating: Option<String>,
    status: Option<String>,
    #[serde(rename = "startDate")]
    start_date: Option<String>,
    season: Option<String>,
    #[serde(rename = "youtubeVideoId")]
    youtube_video_id: Option<String>,
    slug: Option<String>,
}

fn record_to_metadata(record: KitsuRecord, query: &str) -> AnimeMetadata {
    let attributes = record.attributes;
    let title = attributes
        .canonical_title
        .or(attributes.english_title)
        .or(attributes.slug.clone())
        .unwrap_or_else(|| query.to_string());
    let synopsis = attributes.synopsis;
    let score = attributes
        .average_rating
        .and_then(|rating| rating.parse::<f32>().ok());
    let genres = Vec::new();
    let studios = Vec::new();
    let status = attributes.status;
    let season = attributes.season;
    let year = attributes
        .start_date
        .and_then(|date| date.split('-').next().and_then(|n| n.parse().ok()));
    let trailer_url = attributes
        .youtube_video_id
        .map(|id| format!("https://www.youtube.com/watch?v={id}"));
    let source_url = attributes.slug.map_or_else(
        || format!("https://kitsu.io/anime/{}", record.id),
        |slug| format!("https://kitsu.io/anime/{slug}"),
    );
    AnimeMetadata {
        title,
        synopsis,
        score,
        genres,
        studios,
        status,
        season,
        year,
        trailer_url,
        source_url,
        source: MetadataSource::Kitsu,
    }
}
