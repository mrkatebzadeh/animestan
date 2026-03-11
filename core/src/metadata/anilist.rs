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
use serde::{Deserialize, Serialize};

use crate::{AppConfig, error::Error as CoreError};

use super::{
    AniListMetadataProvider, AnimeMetadata, MetadataCache, MetadataProvider, MetadataSource,
    normalize_query,
};

const ANILIST_URL: &str = "https://graphql.anilist.co";
const ANILIST_QUERY: &str = "query ($search: String!) {\n  Media(search: $search, type: ANIME) {\n    title {\n      userPreferred\n      romaji\n      english\n    }\n    description\n    averageScore\n    genres\n    studios(isMain: true) {\n      nodes {\n        name\n      }\n    }\n    status\n    season\n    seasonYear\n    trailer {\n      site\n      id\n      url\n    }\n    siteUrl\n  }\n}\n";

impl MetadataProvider for AniListMetadataProvider {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let key = format!("anilist:{}", normalize_query(query));
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_anilist(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }
}

impl AniListMetadataProvider {
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

    pub(super) fn refresh_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let key = format!("anilist:{}", normalize_query(query));
        let metadata = self.fetch_anilist(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }

    fn fetch_anilist(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let request = AniListGraphQl {
            query: ANILIST_QUERY,
            variables: AniListVariables { search: query },
        };
        let response = self
            .client
            .post(ANILIST_URL)
            .json(&request)
            .send()
            .map_err(|source| CoreError::HttpRequest {
                url: ANILIST_URL.to_string(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::HttpStatus {
                url: ANILIST_URL.to_string(),
                status: status.as_u16(),
            });
        }
        let body = response.text().map_err(|source| CoreError::HttpBodyParse {
            url: ANILIST_URL.to_string(),
            source,
        })?;
        let data = serde_json::from_str::<AniListResponse>(&body).map_err(|source| {
            CoreError::ResponseParse {
                url: ANILIST_URL.to_string(),
                source,
            }
        })?;
        let media = data.data.media.ok_or_else(|| CoreError::MetadataNotFound {
            query: query.to_string(),
        })?;
        Ok(media_to_metadata(media, query))
    }
}

#[derive(Serialize)]
struct AniListGraphQl<'a> {
    query: &'static str,
    variables: AniListVariables<'a>,
}

#[derive(Serialize)]
struct AniListVariables<'a> {
    search: &'a str,
}

#[derive(Deserialize)]
struct AniListResponse {
    data: AniListData,
}

#[derive(Deserialize)]
struct AniListData {
    #[serde(rename = "Media")]
    media: Option<AniListMedia>,
}

#[derive(Deserialize)]
struct AniListMedia {
    title: AniListTitle,
    description: Option<String>,
    #[serde(rename = "averageScore")]
    average_score: Option<f32>,
    genres: Vec<String>,
    studios: AniListStudioConnection,
    status: Option<String>,
    season: Option<String>,
    #[serde(rename = "seasonYear")]
    season_year: Option<i32>,
    trailer: Option<AniListTrailer>,
    #[serde(rename = "siteUrl")]
    site_url: String,
}

#[derive(Deserialize)]
struct AniListTitle {
    #[serde(rename = "userPreferred")]
    user_preferred: Option<String>,
    romaji: Option<String>,
    english: Option<String>,
}

#[derive(Deserialize)]
struct AniListStudioConnection {
    nodes: Vec<AniListStudioNode>,
}

#[derive(Deserialize)]
struct AniListStudioNode {
    name: String,
}

#[derive(Deserialize)]
struct AniListTrailer {
    site: Option<String>,
    id: Option<String>,
    url: Option<String>,
}

fn media_to_metadata(media: AniListMedia, query: &str) -> AnimeMetadata {
    let title = select_title(&media.title).unwrap_or_else(|| query.to_string());
    let synopsis = media.description;
    let score = media.average_score;
    let genres = media.genres;
    let studios = media
        .studios
        .nodes
        .into_iter()
        .map(|node| node.name)
        .collect();
    let status = media.status;
    let season = media.season;
    let year = media
        .season_year
        .and_then(|value| u16::try_from(value).ok());
    let trailer_url = build_trailer_url(media.trailer);
    let source_url = media.site_url;
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
        source: MetadataSource::AniList,
    }
}

fn select_title(title: &AniListTitle) -> Option<String> {
    title
        .user_preferred
        .as_ref()
        .or(title.romaji.as_ref())
        .or(title.english.as_ref())
        .cloned()
}

fn build_trailer_url(trailer: Option<AniListTrailer>) -> Option<String> {
    trailer.and_then(|t| {
        if let Some(url) = t.url {
            return Some(url);
        }
        match (t.site.as_deref(), t.id.as_deref()) {
            (Some("youtube"), Some(id)) => Some(format!("https://www.youtube.com/watch?v={id}")),
            (Some("dailymotion"), Some(id)) => {
                Some(format!("https://www.dailymotion.com/video/{id}"))
            }
            _ => None,
        }
    })
}
