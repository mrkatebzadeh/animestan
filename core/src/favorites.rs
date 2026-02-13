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
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{config::AppConfig, error::Error, models::AnimeEntry};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FavoriteEntry {
    pub anime: AnimeEntry,
    pub added_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct FavoritesStore {
    #[serde(default)]
    entries: HashMap<String, FavoriteEntry>,
}

pub struct FavoriteStore {
    path: PathBuf,
    store: FavoritesStore,
}

impl FavoriteStore {
    /// Loads favorites from the given path, creating an empty store when the
    /// file is missing.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the file cannot be read or if the JSON payload cannot
    /// be parsed into a [`FavoritesStore`].
    pub fn load(path: PathBuf) -> Result<Self, Error> {
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let store: FavoritesStore =
                    serde_json::from_str(&contents).map_err(|source| Error::FavoritesParse {
                        path: path.clone(),
                        source,
                    })?;
                Ok(Self { path, store })
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self {
                path,
                store: FavoritesStore::default(),
            }),
            Err(source) => Err(Error::FavoritesRead { path, source }),
        }
    }

    /// Loads favorites using the default path derived from [`AppConfig`].
    ///
    /// # Errors
    ///
    /// Propagates any errors returned by [`FavoriteStore::load`].
    pub fn load_default(config: &AppConfig) -> Result<Self, Error> {
        Self::load(config.favorites_path())
    }

    #[must_use]
    pub fn list(&self) -> Vec<FavoriteEntry> {
        let mut favorites: Vec<FavoriteEntry> = self.store.entries.values().cloned().collect();
        favorites.sort_by_key(|entry| entry.added_at);
        favorites
    }

    /// Adds an anime entry to the favorites store, persisting it immediately.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the updated store cannot be serialized or written to
    /// disk.
    pub fn add(&mut self, entry: AnimeEntry) -> Result<(), Error> {
        let favorite = FavoriteEntry {
            anime: entry,
            added_at: now_epoch(),
        };
        self.store
            .entries
            .insert(favorite.anime.id.clone(), favorite);
        self.save()
    }

    /// Removes an anime entry from the favorites store by its identifier.
    ///
    /// # Errors
    ///
    /// Returns `Err` if, after removing an existing entry, the updated store
    /// fails to persist to disk.
    pub fn remove(&mut self, anime_id: &str) -> Result<bool, Error> {
        let removed = self.store.entries.remove(anime_id).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> Result<(), Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::FavoritesWrite {
                path: self.path.clone(),
                source,
            })?;
        }

        let mut tmp_path = self.path.clone();
        tmp_path.set_extension("json.tmp");

        let payload =
            serde_json::to_string_pretty(&self.store).map_err(|source| Error::FavoritesWrite {
                path: self.path.clone(),
                source: io::Error::other(source),
            })?;

        fs::write(&tmp_path, payload).map_err(|source| Error::FavoritesWrite {
            path: self.path.clone(),
            source,
        })?;
        fs::rename(&tmp_path, &self.path).map_err(|source| Error::FavoritesWrite {
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
