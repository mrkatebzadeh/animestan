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
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::error::Error as CoreError;

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const ANILIST_URL: &str = "https://graphql.anilist.co";
const KITSU_SEARCH_URL: &str = "https://kitsu.io/api/edge/anime";

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

#[derive(Debug, Clone, Copy)]
pub enum MetadataSource {
    AniList,
    Kitsu,
}

#[derive(Debug, Clone)]
pub struct AnimeMetadata {
    pub title: String,
    pub synopsis: Option<String>,
    pub score: Option<f32>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub status: Option<String>,
    pub season: Option<String>,
    pub year: Option<u16>,
    pub trailer_url: Option<String>,
    pub source_url: String,
    pub source: MetadataSource,
}

/// Provides metadata for the requested query string.
///
/// # Errors
///
/// * `CoreError::MetadataNotFound` if the query cannot be resolved.
/// * `CoreError::HttpRequest`, `CoreError::HttpStatus`, `CoreError::HttpBodyParse`,
///   or `CoreError::ResponseParse` when upstream services fail or return malformed data.
/// * `CoreError::MetadataCacheLock` when the cache mutex cannot be acquired.
pub trait MetadataProvider: Send + Sync {
    /// # Errors
    ///
    /// * `CoreError::MetadataNotFound` if the query cannot be resolved.
    /// * `CoreError::HttpRequest`, `CoreError::HttpStatus`, `CoreError::HttpBodyParse`,
    ///   or `CoreError::ResponseParse` when upstream services fail or return malformed data.
    /// * `CoreError::MetadataCacheLock` when the cache mutex cannot be acquired.
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError>;
}

struct MetadataCache {
    inner: Mutex<HashMap<String, CacheEntry>>,
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

struct CacheEntry {
    metadata: AnimeMetadata,
    created_at: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= CACHE_TTL
    }
}

impl MetadataCache {
    fn get(&self, key: &str) -> Result<Option<AnimeMetadata>, CoreError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        if let Some(entry) = guard.get(key) {
            if entry.is_expired() {
                guard.remove(key);
                return Ok(None);
            }
            return Ok(Some(entry.metadata.clone()));
        }
        Ok(None)
    }

    fn insert(&self, key: String, metadata: AnimeMetadata) -> Result<(), CoreError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        guard.insert(
            key,
            CacheEntry {
                metadata,
                created_at: Instant::now(),
            },
        );
        Ok(())
    }
}

pub struct AniListMetadataProvider {
    client: Client,
    cache: MetadataCache,
}

pub struct KitsuMetadataProvider {
    client: Client,
    cache: MetadataCache,
}

pub struct MetadataResolver {
    primary: AniListMetadataProvider,
    fallback: KitsuMetadataProvider,
}

impl AniListMetadataProvider {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            cache: MetadataCache::default(),
        }
    }
}

impl Default for AniListMetadataProvider {
    fn default() -> Self {
        Self::new(Client::new())
    }
}

impl KitsuMetadataProvider {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            cache: MetadataCache::default(),
        }
    }
}

impl Default for KitsuMetadataProvider {
    fn default() -> Self {
        Self::new(Client::new())
    }
}

impl MetadataResolver {
    #[must_use]
    pub fn new() -> Self {
        Self {
            primary: AniListMetadataProvider::default(),
            fallback: KitsuMetadataProvider::default(),
        }
    }

    #[must_use]
    pub fn with_clients(primary: Client, fallback: Client) -> Self {
        Self {
            primary: AniListMetadataProvider::new(primary),
            fallback: KitsuMetadataProvider::new(fallback),
        }
    }
}

impl Default for MetadataResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataProvider for MetadataResolver {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        match self.primary.fetch_by_query(query) {
            Ok(metadata) => Ok(metadata),
            Err(primary_err) => match self.fallback.fetch_by_query(query) {
                Ok(metadata) => Ok(metadata),
                Err(_) => Err(primary_err),
            },
        }
    }
}

impl MetadataProvider for AniListMetadataProvider {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let key = normalize_query(query);
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_anilist(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }
}

impl MetadataProvider for KitsuMetadataProvider {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        let key = normalize_query(query);
        if let Some(metadata) = self.cache.get(&key)? {
            return Ok(metadata);
        }
        let metadata = self.fetch_kitsu(query)?;
        self.cache.insert(key, metadata.clone())?;
        Ok(metadata)
    }
}

impl AniListMetadataProvider {
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

impl KitsuMetadataProvider {
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

const ANILIST_QUERY: &str = "query ($search: String!) {\n  Media(search: $search, type: ANIME) {\n    title {\n      userPreferred\n      romaji\n      english\n    }\n    description\n    averageScore\n    genres\n    studios(isMain: true) {\n      nodes {\n        name\n      }\n    }\n    status\n    season\n    seasonYear\n    trailer {\n      site\n      id\n      url\n    }\n    siteUrl\n  }\n}\n";
