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

use std::sync::{Arc, Mutex};

use animestan_core::{
    AnimeClient, AppConfig, EpisodeTracker, FavoriteStore, FetchBackend, MetadataResolver,
};
use futures::future::AbortHandle;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::App;
use crate::cache::{EpisodeCache, load_metadata_cache_files, populate_anime_progress_from_cache};
use crate::flow::refresh_episode_indicators;
use crate::tasks::{
    BackgroundEpisodeRefreshResult, MetadataFetchResult, spawn_background_episode_refresh_tasks,
    spawn_background_metadata_refresh_tasks,
};

pub(crate) struct BackgroundRefreshHandles {
    pub(crate) metadata: Vec<AbortHandle>,
    pub(crate) episode: Vec<AbortHandle>,
}

pub(crate) fn initialize_app_state(
    app: &mut App,
    client: &AnimeClient<FetchBackend>,
    favorites: &FavoriteStore,
    episode_cache: &Arc<Mutex<EpisodeCache>>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
) {
    for (anime_id, metadata) in load_metadata_cache_files(config) {
        app.store_metadata(&anime_id, &metadata);
    }
    app.sync_bookmark_cache(favorites);
    if let Ok(cache_guard) = episode_cache.lock() {
        populate_anime_progress_from_cache(app, tracker, &cache_guard);
    }
    initialize_search_state(app, client);
    if let Err(err) = refresh_episode_indicators(app, tracker, config) {
        app.set_details(format!("Failed to refresh indicators: {err}"));
    }
}

pub(crate) fn start_background_refreshes(
    app: &mut App,
    runtime: &Handle,
    resolver: &Arc<MetadataResolver>,
    client: &Arc<AnimeClient<FetchBackend>>,
    episode_cache: &Arc<Mutex<EpisodeCache>>,
    background_job_tx: &UnboundedSender<BackgroundEpisodeRefreshResult>,
    metadata_result_tx: &UnboundedSender<MetadataFetchResult>,
) -> BackgroundRefreshHandles {
    let background_metadata_targets: Vec<(String, String)> = app
        .bookmark_entries()
        .iter()
        .map(|entry| (entry.anime.id.clone(), entry.anime.title.clone()))
        .collect();
    let background_episode_targets: Vec<String> = app
        .bookmark_entries()
        .iter()
        .map(|entry| entry.anime.id.clone())
        .collect();
    let total_background_jobs =
        background_metadata_targets.len() + background_episode_targets.len();
    if total_background_jobs > 0 {
        app.start_metadata_background_refresh(total_background_jobs);
    }
    if !background_episode_targets.is_empty() {
        for anime_id in &background_episode_targets {
            app.mark_episode_refresh_pending(anime_id.clone());
        }
    }

    let metadata = if background_metadata_targets.is_empty() {
        Vec::new()
    } else {
        spawn_background_metadata_refresh_tasks(
            runtime,
            resolver,
            background_metadata_targets,
            metadata_result_tx,
        )
    };
    let episode = if background_episode_targets.is_empty() {
        Vec::new()
    } else {
        spawn_background_episode_refresh_tasks(
            runtime,
            client,
            background_episode_targets,
            episode_cache,
            background_job_tx.clone(),
        )
    };

    BackgroundRefreshHandles { metadata, episode }
}

fn initialize_search_state(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if app.search_query().trim().is_empty() {
        app.set_details("Press / to search for an anime.");
    } else if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
}
