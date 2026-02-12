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

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{config::AppConfig, error::Error};

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
    pub fn load(path: PathBuf) -> Result<Self, Error> {
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let store: ProgressStore =
                    serde_json::from_str(&contents).map_err(|source| Error::TrackingParse {
                        path: path.clone(),
                        source,
                    })?;
                Ok(Self { path, store })
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self {
                path,
                store: ProgressStore::default(),
            }),
            Err(source) => Err(Error::TrackingRead { path, source }),
        }
    }

    /// Loads tracking data from the path derived via [`AppConfig::progress_path`].
    ///
    /// # Errors
    ///
    /// Propagates all errors from [`EpisodeTracker::load`].
    pub fn load_default(config: &AppConfig) -> Result<Self, Error> {
        Self::load(config.progress_path())
    }

    /// Marks an episode as started and persists the tracker state.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TrackingWrite`] if the progress file cannot be written.
    pub fn mark_started(&mut self, episode_id: &str) -> Result<(), Error> {
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
    ) -> Result<(), Error> {
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
    pub fn mark_watched(&mut self, episode_id: &str) -> Result<(), Error> {
        let entry = self
            .store
            .episodes
            .entry(episode_id.to_string())
            .or_default();
        entry.watched = true;
        entry.updated_at = now_epoch();
        self.save()
    }

    fn save(&self) -> Result<(), Error> {
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
        Ok(())
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
