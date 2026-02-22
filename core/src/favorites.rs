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

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    CoreResult,
    config::AppConfig,
    error::Error,
    models::AnimeEntry,
    store::{load_json_or_default, now_epoch, save_json_pretty},
};

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
    pub fn load(path: PathBuf) -> CoreResult<Self> {
        let store = load_json_or_default(
            &path,
            |path, source| Error::FavoritesParse { path, source },
            |path, source| Error::FavoritesRead { path, source },
        )?;
        Ok(Self { path, store })
    }

    /// Loads favorites using the default path derived from [`AppConfig`].
    ///
    /// # Errors
    ///
    /// Propagates any errors returned by [`FavoriteStore::load`].
    pub fn load_default(config: &AppConfig) -> CoreResult<Self> {
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
    pub fn add(&mut self, entry: AnimeEntry) -> CoreResult<()> {
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
    pub fn remove(&mut self, anime_id: &str) -> CoreResult<bool> {
        let removed = self.store.entries.remove(anime_id).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> CoreResult<()> {
        save_json_pretty(&self.path, &self.store, |path, source| {
            Error::FavoritesWrite { path, source }
        })
    }
}
