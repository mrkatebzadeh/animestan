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
    AniDbMetadataProvider, AnimeClient, AppConfig, EpisodeTracker, FavoriteStore, FetchBackend,
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
    resolver: &Arc<AniDbMetadataProvider>,
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
    prime_background_refresh_state(
        app,
        &background_metadata_targets,
        &background_episode_targets,
    );

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

fn prime_background_refresh_state(
    app: &mut App,
    background_metadata_targets: &[(String, String)],
    background_episode_targets: &[String],
) {
    let total_background_jobs =
        background_metadata_targets.len() + background_episode_targets.len();
    if total_background_jobs > 0 {
        app.start_metadata_background_refresh(total_background_jobs);
    }
    for (anime_id, _) in background_metadata_targets {
        app.mark_metadata_pending(anime_id.clone());
    }
    if !background_episode_targets.is_empty() {
        for anime_id in background_episode_targets {
            app.mark_episode_refresh_pending(anime_id.clone());
        }
    }
}

fn initialize_search_state(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if app.search_query().trim().is_empty() {
        app.set_details("Press / to search for an anime.");
    } else if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
}

#[cfg(test)]
mod tests {
    use super::prime_background_refresh_state;
    use crate::app::App;
    use animestan_core::{AnimeEntry, FavoriteStore};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_path(name: &str) -> String {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should advance")
            .as_nanos();
        let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!(
                "animestan-bootstrap-{name}-{}-{stamp}-{counter}.json",
                std::process::id(),
            ))
            .display()
            .to_string()
    }

    fn test_config() -> animestan_core::AppConfig {
        animestan_core::AppConfig {
            favorites_path: Some(unique_temp_path("favorites")),
            tracking_path: Some(unique_temp_path("tracking")),
            metadata_cache_path: Some(unique_temp_path("metadata-cache")),
            episodes_cache_path: Some(unique_temp_path("episodes-cache")),
            ..Default::default()
        }
    }

    fn sample_bookmarks() -> FavoriteStore {
        let config = test_config();
        let mut favorites =
            FavoriteStore::load(config.favorites_path()).expect("favorites store should load");
        favorites
            .add(AnimeEntry {
                id: "naruto".to_string(),
                title: "Naruto".to_string(),
                source_id: "anidb".to_string(),
            })
            .expect("favorite should persist");
        favorites
            .add(AnimeEntry {
                id: "bleach".to_string(),
                title: "Bleach".to_string(),
                source_id: "anidb".to_string(),
            })
            .expect("favorite should persist");
        favorites
    }

    #[test]
    fn priming_background_refreshes_blocks_duplicate_list_metadata_fetches() {
        let favorites = sample_bookmarks();
        let mut app = App::new();
        app.load_bookmarks(&favorites);
        let _ = app.take_anime_selection_changed();
        let metadata_targets: Vec<(String, String)> = app
            .bookmark_entries()
            .iter()
            .map(|entry| (entry.anime.id.clone(), entry.anime.title.clone()))
            .collect();
        let episode_targets: Vec<String> = app
            .bookmark_entries()
            .iter()
            .map(|entry| entry.anime.id.clone())
            .collect();

        prime_background_refresh_state(&mut app, &metadata_targets, &episode_targets);

        assert!(app.next_metadata_fetch_candidate().is_none());
    }
}
