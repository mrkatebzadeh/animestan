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

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use spdlog::prelude::*;

use crate::{
    CoreResult,
    config::AppConfig,
    error::Error,
    models::{Episode, EpisodePlaybackState, PlaybackFilter},
};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct EpisodeProgress {
    pub watched: bool,
    pub last_position_sec: Option<f64>,
    pub duration_sec: Option<f64>,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ProgressStore {
    #[serde(default)]
    episodes: HashMap<String, EpisodeProgress>,
}

pub struct EpisodeTracker {
    path: PathBuf,
    store: ProgressStore,
}

impl EpisodeTracker {
    /// Loads tracking data from `path`, creating a new store when the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TrackingRead`] when the file cannot be read or
    /// [`Error::TrackingParse`] when its contents cannot be decoded.
    pub fn load(path: PathBuf) -> CoreResult<Self> {
        match fs::read_to_string(&path) {
            Ok(contents) => {
                debug!("loaded playback progress from {}", path.display());
                let store: ProgressStore =
                    serde_json::from_str(&contents).map_err(|source| Error::TrackingParse {
                        path: path.clone(),
                        source,
                    })?;
                Ok(Self { path, store })
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                debug!(
                    "no playback progress file at {}, starting empty",
                    path.display()
                );
                Ok(Self {
                    path,
                    store: ProgressStore::default(),
                })
            }
            Err(source) => Err(Error::TrackingRead { path, source }.into()),
        }
    }

    /// Loads tracking data from the path derived via [`AppConfig::progress_path`].
    ///
    /// # Errors
    ///
    /// Propagates all errors from [`EpisodeTracker::load`].
    pub fn load_default(config: &AppConfig) -> CoreResult<Self> {
        Self::load(config.progress_path())
    }

    /// Marks an episode as started and persists the tracker state.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TrackingWrite`] if the progress file cannot be written.
    pub fn mark_started(&mut self, episode_id: &str) -> CoreResult<()> {
        let entry = self
            .store
            .episodes
            .entry(episode_id.to_string())
            .or_default();
        entry.updated_at = now_epoch();
        self.save()
    }

    /// Updates playback progress details for an episode.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TrackingWrite`] if the progress file cannot be written.
    pub fn update_progress(
        &mut self,
        episode_id: &str,
        position: f64,
        duration: Option<f64>,
    ) -> CoreResult<()> {
        let entry = self
            .store
            .episodes
            .entry(episode_id.to_string())
            .or_default();
        entry.last_position_sec = Some(position);
        if let Some(duration) = duration {
            entry.duration_sec = Some(duration);
        }
        entry.updated_at = now_epoch();
        self.save()
    }

    /// Marks an episode as fully watched and persists the tracker state.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TrackingWrite`] if the progress file cannot be written.
    pub fn mark_watched(&mut self, episode_id: &str) -> CoreResult<()> {
        let entry = self
            .store
            .episodes
            .entry(episode_id.to_string())
            .or_default();
        entry.watched = true;
        entry.updated_at = now_epoch();
        info!("marked '{episode_id}' as watched");
        self.save()
    }

    /// Returns the persisted playback state for an episode, when available.
    #[must_use]
    pub fn state_for(&self, episode_id: &str) -> Option<EpisodePlaybackState> {
        self.store
            .episodes
            .get(episode_id)
            .map(|entry| EpisodePlaybackState {
                watched: entry.watched,
                in_progress: !entry.watched && entry.last_position_sec.is_some(),
                updated_at: entry.updated_at,
            })
    }

    /// Indicates whether the episode has been marked as watched.
    #[must_use]
    pub fn is_watched(&self, episode_id: &str) -> bool {
        self.store
            .episodes
            .get(episode_id)
            .is_some_and(|entry| entry.watched)
    }

    /// Indicates whether playback progress exists without being fully watched.
    #[must_use]
    pub fn is_in_progress(&self, episode_id: &str) -> bool {
        self.store
            .episodes
            .get(episode_id)
            .is_some_and(|entry| !entry.watched && entry.last_position_sec.is_some())
    }

    /// Filters episodes by the requested playback filter.
    #[must_use]
    pub fn filter_episodes(&self, episodes: &[Episode], filter: PlaybackFilter) -> Vec<Episode> {
        match filter {
            PlaybackFilter::Unwatched => episodes
                .iter()
                .filter(|episode| !self.is_watched(&episode.id))
                .cloned()
                .collect(),
            PlaybackFilter::InProgress => episodes
                .iter()
                .filter(|episode| self.is_in_progress(&episode.id))
                .cloned()
                .collect(),
            PlaybackFilter::Next => episodes
                .iter()
                .filter(|episode| !self.is_watched(&episode.id))
                .min_by_key(|episode| episode.number)
                .cloned()
                .into_iter()
                .collect(),
            PlaybackFilter::Recent => {
                let mut with_state: Vec<(u64, Episode)> = episodes
                    .iter()
                    .filter_map(|episode| {
                        self.state_for(&episode.id)
                            .map(|state| (state.updated_at, episode.clone()))
                    })
                    .collect();
                with_state.sort_by(|a, b| b.0.cmp(&a.0));
                with_state.into_iter().map(|(_, episode)| episode).collect()
            }
        }
    }

    /// Returns the most recently updated episode among the provided list.
    #[must_use]
    pub fn most_recent_episode(&self, episodes: &[Episode]) -> Option<Episode> {
        episodes
            .iter()
            .filter_map(|episode| {
                self.state_for(&episode.id)
                    .map(|state| (state.updated_at, episode))
            })
            .max_by_key(|(updated_at, _)| *updated_at)
            .map(|(_, episode)| episode.clone())
    }

    fn save(&self) -> CoreResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::TrackingWrite {
                path: self.path.clone(),
                source,
            })?;
        }

        let tmp_path = Path::new(&self.path).with_extension("json.tmp");
        let payload =
            serde_json::to_string_pretty(&self.store).map_err(|source| Error::TrackingWrite {
                path: self.path.clone(),
                source: io::Error::other(source),
            })?;
        fs::write(&tmp_path, payload).map_err(|source| Error::TrackingWrite {
            path: self.path.clone(),
            source,
        })?;
        fs::rename(&tmp_path, &self.path).map_err(|source| Error::TrackingWrite {
            path: self.path.clone(),
            source,
        })?;
        debug!("saved playback progress to {}", self.path.display());
        Ok(())
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
