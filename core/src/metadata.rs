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

use serde::{Deserialize, Serialize};

use crate::{error::Error as CoreError, store::now_epoch};

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
                serde_json::from_str::<MetadataCacheFile>(&contents).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::{MetadataCache, MetadataSource};

    #[test]
    fn anidb_source_serializes_in_lowercase() {
        assert_eq!(
            serde_json::to_string(&MetadataSource::AniDb).expect("source should serialize"),
            "\"anidb\""
        );
    }

    #[test]
    fn incompatible_cache_is_preserved_and_ignored() {
        let path = std::env::temp_dir().join(format!(
            "animestan-metadata-incompatible-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let contents = r#"{"entries":{"legacy":{"metadata":{"title":"Legacy"}}}}"#;
        std::fs::write(&path, contents).expect("write incompatible cache");

        let cache = MetadataCache::new(path.clone());

        assert!(cache.get("anidb:id:legacy").expect("cache read").is_none());
        assert_eq!(
            std::fs::read_to_string(&path).expect("read cache"),
            contents
        );
        std::fs::remove_file(path).expect("remove test cache");
    }
}
