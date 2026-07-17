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
use std::sync::{Arc, Mutex};

use animestan_core::{AnimeMetadata, AppConfig, Episode, EpisodeTracker};
use anyhow::{Result, anyhow};
use image::{DynamicImage, ImageFormat};

use crate::app::{AnimeProgress, App};

pub(crate) struct EpisodeCache {
    dir: PathBuf,
}

pub(crate) struct CoverCache {
    dir: PathBuf,
}

impl EpisodeCache {
    pub(crate) fn load(config: &AppConfig) -> Self {
        let dir = config.episodes_cache_path().with_file_name("episodes");
        Self { dir }
    }

    pub(crate) fn get(&self, anime_id: &str) -> Option<Vec<Episode>> {
        let path = self.dir.join(format!("{anime_id}.json"));
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str::<Vec<Episode>>(&contents).ok())
    }

    pub(crate) fn insert(&mut self, anime_id: &str, episodes: &[Episode]) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let payload = serde_json::to_string_pretty(episodes)?;
        let path = self.dir.join(format!("{anime_id}.json"));
        std::fs::write(path, payload)?;
        Ok(())
    }
}

impl CoverCache {
    pub(crate) fn load(config: &AppConfig) -> Self {
        let dir = config.covers_dir();
        Self { dir }
    }

    fn path_for(&self, anime_id: &str) -> PathBuf {
        self.dir.join(format!("{anime_id}.png"))
    }

    pub(crate) fn get(&self, anime_id: &str) -> Result<Option<DynamicImage>> {
        let path = self.path_for(anime_id);
        if !path.is_file() {
            return Ok(None);
        }
        let image = image::open(path)?;
        Ok(Some(image))
    }

    pub(crate) fn insert(&self, anime_id: &str, image: &DynamicImage) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path_for(anime_id);
        image.save_with_format(path, ImageFormat::Png)?;
        Ok(())
    }
}

pub(crate) fn cached_episodes(
    cache: &Arc<Mutex<EpisodeCache>>,
    anime_id: &str,
) -> Result<Option<Vec<Episode>>> {
    let guard = cache
        .lock()
        .map_err(|_| anyhow!("episode cache lock poisoned"))?;
    Ok(guard.get(anime_id))
}

pub(crate) fn cache_episodes(
    cache: &Arc<Mutex<EpisodeCache>>,
    anime_id: &str,
    episodes: &[Episode],
) -> Result<()> {
    let mut guard = cache
        .lock()
        .map_err(|_| anyhow!("episode cache lock poisoned"))?;
    guard.insert(anime_id, episodes)
}

fn metadata_cache_dir(config: &AppConfig) -> PathBuf {
    config.metadata_cache_path().with_file_name("metadata")
}

fn metadata_cache_file(config: &AppConfig, anime_id: &str) -> PathBuf {
    metadata_cache_dir(config).join(format!("{anime_id}.json"))
}

pub(crate) fn load_metadata_cache_files(config: &AppConfig) -> HashMap<String, AnimeMetadata> {
    let mut entries = HashMap::new();
    let dir = metadata_cache_dir(config);
    let Ok(read_dir) = std::fs::read_dir(&dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(metadata) = serde_json::from_str::<AnimeMetadata>(&contents) {
                entries.insert(stem.to_string(), metadata);
            }
        }
    }
    entries
}

pub(crate) fn save_metadata_cache_file(
    config: &AppConfig,
    anime_id: &str,
    metadata: &AnimeMetadata,
) -> Result<()> {
    let dir = metadata_cache_dir(config);
    std::fs::create_dir_all(&dir)?;
    let path = metadata_cache_file(config, anime_id);
    let payload = serde_json::to_string_pretty(metadata)?;
    std::fs::write(path, payload)?;
    Ok(())
}

pub(crate) fn populate_anime_progress_from_cache(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    cache: &EpisodeCache,
) {
    if let Ok(guard) = tracker.lock() {
        let bookmark_ids: Vec<String> = app
            .bookmark_entries()
            .iter()
            .map(|entry| entry.anime.id.clone())
            .collect();
        for anime_id in bookmark_ids {
            if let Some(episodes) = cache.get(&anime_id) {
                let watched = episodes
                    .iter()
                    .filter(|episode| {
                        guard
                            .state_for(&episode.id)
                            .as_ref()
                            .is_some_and(|state| state.watched)
                    })
                    .count();
                app.set_anime_progress(
                    anime_id.clone(),
                    AnimeProgress {
                        watched,
                        total: episodes.len(),
                    },
                );
            }
        }
    }
}
