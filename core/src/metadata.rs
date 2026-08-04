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
use std::path::PathBuf;
use std::sync::Mutex;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::{AppConfig, error::Error as CoreError, store::now_epoch};

mod anidb;

pub use anidb::AniDbMetadataProvider;

const CACHE_TTL_SECS: u64 = 5 * 60;

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataSource {
    AniDb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub image_url: Option<String>,
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

#[derive(Clone, Debug)]
struct MetadataCache {
    inner: std::sync::Arc<MetadataCacheInner>,
}

#[derive(Debug)]
struct MetadataCacheInner {
    entries: Mutex<HashMap<String, CacheEntry>>,
    loaded: Mutex<bool>,
    path: PathBuf,
}

fn default_epoch() -> u64 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    metadata: AnimeMetadata,
    #[serde(default = "default_epoch")]
    created_at: u64,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        now_epoch().saturating_sub(self.created_at) >= CACHE_TTL_SECS
    }
}

impl MetadataCache {
    fn new(path: PathBuf) -> Self {
        Self {
            inner: std::sync::Arc::new(MetadataCacheInner {
                entries: Mutex::new(HashMap::new()),
                loaded: Mutex::new(false),
                path,
            }),
        }
    }

    fn get(&self, key: &str) -> Result<Option<AnimeMetadata>, CoreError> {
        self.load_if_needed()?;
        let mut guard = self
            .inner
            .entries
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        if let Some(entry) = guard.get(key) {
            if entry.is_expired() {
                guard.remove(key);
                drop(guard);
                self.save()?;
                return Ok(None);
            }
            return Ok(Some(entry.metadata.clone()));
        }
        Ok(None)
    }

    fn insert(&self, key: String, metadata: AnimeMetadata) -> Result<(), CoreError> {
        self.load_if_needed()?;
        let mut guard = self
            .inner
            .entries
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        guard.insert(
            key,
            CacheEntry {
                metadata,
                created_at: now_epoch(),
            },
        );
        drop(guard);
        self.save()?;
        Ok(())
    }

    fn load_if_needed(&self) -> Result<(), CoreError> {
        let mut loaded = self
            .inner
            .loaded
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        if *loaded {
            return Ok(());
        }

        let path = self.inner.path.clone();
        let cache = match std::fs::read_to_string(&path) {
            Ok(contents) => {
                if let Ok(cache) = serde_json::from_str::<MetadataCacheFile>(&contents) {
                    cache
                } else {
                    let _ = std::fs::remove_file(&path);
                    MetadataCacheFile::default()
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => MetadataCacheFile::default(),
            Err(source) => {
                return Err(CoreError::MetadataCacheRead {
                    path: path.clone(),
                    source,
                });
            }
        };
        let mut guard = self
            .inner
            .entries
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        guard.clear();
        guard.extend(
            cache
                .entries
                .into_iter()
                .filter(|(_, entry)| !entry.is_expired()),
        );
        *loaded = true;
        Ok(())
    }

    fn save(&self) -> Result<(), CoreError> {
        let guard = self
            .inner
            .entries
            .lock()
            .map_err(|_| CoreError::MetadataCacheLock)?;
        let cache = MetadataCacheFile {
            entries: guard.clone(),
        };
        if let Some(parent) = self.inner.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CoreError::MetadataCacheWrite {
                path: self.inner.path.clone(),
                source,
            })?;
        }
        let payload = serde_json::to_string_pretty(&cache).map_err(|source| {
            CoreError::MetadataCacheWrite {
                path: self.inner.path.clone(),
                source: std::io::Error::other(source),
            }
        })?;
        std::fs::write(&self.inner.path, payload).map_err(|source| CoreError::MetadataCacheWrite {
            path: self.inner.path.clone(),
            source,
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataCacheFile {
    #[serde(default)]
    entries: HashMap<String, CacheEntry>,
}

pub struct MetadataResolver {
    anidb: AniDbMetadataProvider,
}

impl MetadataResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::with_cache(AppConfig::default().metadata_cache_path())
    }

    #[must_use]
    pub fn from_config(config: &AppConfig) -> Self {
        Self::with_cache(config.metadata_cache_path())
    }

    fn with_cache(cache_path: PathBuf) -> Self {
        Self {
            anidb: AniDbMetadataProvider::with_cache(Client::new(), MetadataCache::new(cache_path)),
        }
    }

    /// Fetches metadata using an `AniDB` identifier when available.
    ///
    /// # Errors
    ///
    /// Returns an error if `AniDB` cannot resolve the identifier or the upstream response cannot be
    /// parsed.
    pub fn fetch_by_id(&self, id: &str, query: &str) -> Result<AnimeMetadata, CoreError> {
        self.anidb.fetch_by_id(id, query)
    }

    /// Refreshes metadata by query, bypassing the cache.
    ///
    /// # Errors
    ///
    /// Returns an error if `AniDB` cannot resolve the query or the upstream response cannot be
    /// parsed.
    pub fn refresh_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        self.anidb.refresh_by_query(query)
    }

    /// Refreshes metadata by `AniDB` identifier, bypassing the cache.
    ///
    /// # Errors
    ///
    /// Returns an error if `AniDB` cannot resolve the identifier or the upstream response cannot be
    /// parsed.
    pub fn refresh_by_id(&self, id: &str, query: &str) -> Result<AnimeMetadata, CoreError> {
        self.anidb.refresh_by_id(id, query)
    }
}

impl Default for MetadataResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataProvider for MetadataResolver {
    fn fetch_by_query(&self, query: &str) -> Result<AnimeMetadata, CoreError> {
        self.anidb.fetch_by_query(query)
    }
}

#[cfg(test)]
mod tests {
    use super::MetadataSource;

    #[test]
    fn anidb_source_serializes_in_lowercase() {
        assert_eq!(
            serde_json::to_string(&MetadataSource::AniDb).expect("source should serialize"),
            "\"anidb\""
        );
    }
}
