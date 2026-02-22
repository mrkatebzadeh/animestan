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

use crate::error::Error as CoreError;

mod allmanga;
mod anilist;
mod kitsu;

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

#[derive(Debug, Clone, Copy)]
pub enum MetadataSource {
    AllManga,
    AniList,
    Kitsu,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum MetadataProviderKind {
    #[default]
    AllManga,
    AniList,
    Kitsu,
}

impl MetadataProviderKind {
    #[must_use]
    pub fn from_config(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("anilist") => Self::AniList,
            Some("kitsu") => Self::Kitsu,
            Some("allmanga") => Self::AllManga,
            _ => Self::default(),
        }
    }
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

pub struct AllMangaMetadataProvider {
    client: Client,
    cache: MetadataCache,
}

pub struct KitsuMetadataProvider {
    client: Client,
    cache: MetadataCache,
}

pub struct MetadataResolver {
    primary_kind: MetadataProviderKind,
    fallback_kind: MetadataProviderKind,
    allmanga: AllMangaMetadataProvider,
    anilist: AniListMetadataProvider,
    kitsu: KitsuMetadataProvider,
}

impl Default for AniListMetadataProvider {
    fn default() -> Self {
        Self::new(Client::new())
    }
}

impl Default for AllMangaMetadataProvider {
    fn default() -> Self {
        Self::new(Client::new())
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
        Self::with_primary(MetadataProviderKind::default())
    }

    #[must_use]
    pub fn with_clients(primary: Client, fallback: Client) -> Self {
        let primary_kind = MetadataProviderKind::default();
        let fallback_kind = fallback_for(primary_kind);
        Self {
            primary_kind,
            fallback_kind,
            allmanga: AllMangaMetadataProvider::default(),
            anilist: AniListMetadataProvider::new(primary),
            kitsu: KitsuMetadataProvider::new(fallback),
        }
    }

    #[must_use]
    pub fn from_config(config: &crate::AppConfig) -> Self {
        let primary = MetadataProviderKind::from_config(config.metadata_source.as_deref());
        Self::with_primary(primary)
    }

    #[must_use]
    pub fn with_primary(primary_kind: MetadataProviderKind) -> Self {
        let fallback_kind = fallback_for(primary_kind);
        Self {
            primary_kind,
            fallback_kind,
            allmanga: AllMangaMetadataProvider::default(),
            anilist: AniListMetadataProvider::default(),
            kitsu: KitsuMetadataProvider::default(),
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
        match self.fetch_with(self.primary_kind, query) {
            Ok(metadata) => Ok(metadata),
            Err(primary_err) => match self.fetch_with(self.fallback_kind, query) {
                Ok(metadata) => Ok(metadata),
                Err(_) => Err(primary_err),
            },
        }
    }
}

impl MetadataResolver {
    /// Fetches metadata using a provider-specific identifier when available.
    ///
    /// # Errors
    ///
    /// * `CoreError::MetadataNotFound` if the identifier cannot be resolved.
    /// * `CoreError::HttpRequest`, `CoreError::HttpStatus`, `CoreError::HttpBodyParse`,
    ///   or `CoreError::ResponseParse` when upstream services fail or return malformed data.
    /// * `CoreError::MetadataCacheLock` when the cache mutex cannot be acquired.
    pub fn fetch_by_id(&self, id: &str, query: &str) -> Result<AnimeMetadata, CoreError> {
        match self.primary_kind {
            MetadataProviderKind::AllManga => match self.allmanga.fetch_by_id(id, query) {
                Ok(metadata) => Ok(metadata),
                Err(primary_err) => match self.fetch_with(self.fallback_kind, query) {
                    Ok(metadata) => Ok(metadata),
                    Err(_) => Err(primary_err),
                },
            },
            _ => self.fetch_by_query(query),
        }
    }

    fn fetch_with(
        &self,
        kind: MetadataProviderKind,
        query: &str,
    ) -> Result<AnimeMetadata, CoreError> {
        match kind {
            MetadataProviderKind::AllManga => self.allmanga.fetch_by_query(query),
            MetadataProviderKind::AniList => self.anilist.fetch_by_query(query),
            MetadataProviderKind::Kitsu => self.kitsu.fetch_by_query(query),
        }
    }
}

fn fallback_for(primary: MetadataProviderKind) -> MetadataProviderKind {
    match primary {
        MetadataProviderKind::Kitsu => MetadataProviderKind::AniList,
        MetadataProviderKind::AniList | MetadataProviderKind::AllManga => {
            MetadataProviderKind::Kitsu
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MetadataProviderKind;

    #[test]
    fn provider_kind_defaults_to_allmanga() {
        assert_eq!(
            MetadataProviderKind::from_config(None),
            MetadataProviderKind::AllManga
        );
    }

    #[test]
    fn provider_kind_parses_strings() {
        assert_eq!(
            MetadataProviderKind::from_config(Some("anilist")),
            MetadataProviderKind::AniList
        );
        assert_eq!(
            MetadataProviderKind::from_config(Some("kitsu")),
            MetadataProviderKind::Kitsu
        );
        assert_eq!(
            MetadataProviderKind::from_config(Some("allmanga")),
            MetadataProviderKind::AllManga
        );
        assert_eq!(
            MetadataProviderKind::from_config(Some("unknown")),
            MetadataProviderKind::AllManga
        );
    }
}
