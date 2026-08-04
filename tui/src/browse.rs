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

use animestan_core::{AnimeClient, AppConfig, EpisodeTracker, FetchBackend, MetadataResolver};
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::App;
use crate::cache::{CoverCache, EpisodeCache, cached_episodes};
use crate::flow::{apply_episode_filter, refresh_episode_indicators};
use crate::media::{ActiveMetadataFetch, ImageLoadRequest, queue_image_load};
use crate::tasks::{
    EpisodeFetchRequest, MetadataFetchRequest, MetadataFetchResult, MetadataTarget,
    spawn_metadata_fetch_task,
};

pub(crate) fn handle_search(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if !app.take_pending_search() {
        return;
    }

    if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_current_anime_refresh(
    app: &mut App,
    runtime: &Handle,
    resolver: &Arc<MetadataResolver>,
    metadata_result_tx: &UnboundedSender<MetadataFetchResult>,
    active_metadata_fetch: &mut Option<ActiveMetadataFetch>,
    request_tx: &UnboundedSender<EpisodeFetchRequest>,
) {
    let Some(refresh) = app.take_pending_anime_refresh() else {
        return;
    };

    app.set_details(format!(
        "Refreshing metadata and episodes for {}...",
        refresh.title
    ));

    request_episode_refresh(app, request_tx, &refresh.anime_id);
    request_metadata_refresh(
        app,
        runtime,
        resolver,
        metadata_result_tx,
        active_metadata_fetch,
        refresh,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_filters(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
    request_tx: &UnboundedSender<EpisodeFetchRequest>,
    episode_cache: &Arc<Mutex<EpisodeCache>>,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
) {
    if app.take_anime_selection_changed() {
        if let Some(anime_id) = app.current_anime_id() {
            app.record_anime_history(&anime_id);
            let cached = match cached_episodes(episode_cache, &anime_id) {
                Ok(cached) => cached,
                Err(err) => {
                    app.set_details(format!("Failed to access cached episodes: {err}"));
                    None
                }
            };
            let has_cached = cached.is_some();
            if let Some(cached) = cached {
                app.set_episodes(cached);
                app.set_details("Loaded cached episodes; refreshing...");
                if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                    app.set_details(format!("Failed to refresh indicators: {err}"));
                }
            }
            let should_fetch = !app.episode_refresh_pending(&anime_id);
            if should_fetch {
                app.set_episodes_loading(true);
                app.mark_episode_refresh_pending(anime_id.clone());
                let generation = app.next_fetch_generation();
                let request = EpisodeFetchRequest {
                    generation,
                    anime_id: anime_id.clone(),
                };
                if request_tx.send(request).is_err() {
                    app.set_episodes_loading(false);
                    app.clear_episode_refresh_pending(&anime_id);
                    app.set_details("Episode fetch queue unavailable.");
                } else if has_cached {
                    app.set_details("Refreshing cached episodes...");
                } else {
                    app.set_details("Fetching episodes...");
                }
            }
            let image_url = app
                .cached_metadata_for_current_anime()
                .and_then(|metadata| metadata.image_url.clone());
            if let Some(image_url) = image_url {
                queue_image_load(app, cover_cache, image_request_tx, &anime_id, &image_url);
            }
            app.request_info_metadata();
        } else {
            app.clear_episodes();
            app.set_episodes_loading(false);
            app.set_details("Select an anime to load episodes.");
        }
    }

    if app.take_filter_changed()
        && let Err(err) = apply_episode_filter(app, tracker)
    {
        app.set_details(format!("Filter failed: {err}"));
    }
}

fn request_episode_refresh(
    app: &mut App,
    request_tx: &UnboundedSender<EpisodeFetchRequest>,
    anime_id: &str,
) {
    app.set_episodes_loading(true);
    app.mark_episode_refresh_pending(anime_id.to_string());
    let generation = app.next_fetch_generation();
    let request = EpisodeFetchRequest {
        generation,
        anime_id: anime_id.to_string(),
    };
    if request_tx.send(request).is_err() {
        app.set_episodes_loading(false);
        app.clear_episode_refresh_pending(anime_id);
        app.set_details("Episode fetch queue unavailable.");
    }
}

fn request_metadata_refresh(
    app: &mut App,
    runtime: &Handle,
    resolver: &Arc<MetadataResolver>,
    metadata_result_tx: &UnboundedSender<MetadataFetchResult>,
    active_metadata_fetch: &mut Option<ActiveMetadataFetch>,
    refresh: crate::app::AnimeRefreshRequest,
) {
    if let Some(active_fetch) = active_metadata_fetch.take() {
        if matches!(active_fetch.target, MetadataTarget::List)
            && let Some(anime_id) = active_fetch.anime_id.as_deref()
        {
            app.clear_metadata_pending(anime_id);
        }
        active_fetch.handle.abort();
    }
    let anime_id = refresh.anime_id;

    let request = MetadataFetchRequest {
        generation: app.next_manual_metadata_generation(),
        query: refresh.title,
        source_id: Some(anime_id.clone()),
        anime_id: Some(anime_id.clone()),
        target: MetadataTarget::CurrentRefresh,
        force_refresh: true,
    };

    let abort_handle = spawn_metadata_fetch_task(
        runtime,
        Arc::clone(resolver),
        request,
        metadata_result_tx.clone(),
    );
    *active_metadata_fetch = Some(ActiveMetadataFetch {
        anime_id: Some(anime_id),
        target: MetadataTarget::CurrentRefresh,
        handle: abort_handle,
    });
}

#[cfg(test)]
mod tests {
    use super::request_metadata_refresh;
    use crate::app::{AnimeRefreshRequest, App};
    use crate::media::ActiveMetadataFetch;
    use crate::tasks::MetadataTarget;
    use animestan_core::{
        AnimeEntry, AnimeMetadata, FavoriteStore, MetadataResolver, MetadataSource,
    };
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::runtime::Builder;
    use tokio::sync::mpsc::unbounded_channel;

    fn unique_temp_path(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should advance")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "animestan-browse-{name}-{}-{stamp}.json",
            std::process::id()
        ))
    }

    fn app_with_two_bookmarks() -> App {
        let mut store =
            FavoriteStore::load(unique_temp_path("favorites")).expect("store should load");
        store
            .add(AnimeEntry {
                id: "naruto".to_string(),
                title: "Naruto".to_string(),
                source_id: "anidb".to_string(),
            })
            .expect("bookmark should persist");
        store
            .add(AnimeEntry {
                id: "bleach".to_string(),
                title: "Bleach".to_string(),
                source_id: "anidb".to_string(),
            })
            .expect("bookmark should persist");

        let mut app = App::new();
        app.load_bookmarks(&store);
        let _ = app.take_anime_selection_changed();
        app
    }

    fn sample_metadata(title: &str) -> AnimeMetadata {
        AnimeMetadata {
            title: title.to_string(),
            synopsis: None,
            score: None,
            genres: Vec::new(),
            studios: Vec::new(),
            status: None,
            season: None,
            year: None,
            trailer_url: None,
            image_url: None,
            source_url: format!("https://example.com/{title}"),
            source: MetadataSource::AniDb,
        }
    }

    #[test]
    fn manual_refresh_requeues_aborted_list_metadata_fetch() {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let resolver = Arc::new(MetadataResolver::default());
        let mut app = app_with_two_bookmarks();
        let first_candidate = app
            .next_metadata_fetch_candidate()
            .expect("first list metadata candidate should exist");
        let aborted_anime_id = first_candidate.0.clone();
        app.move_down();
        let _ = app.take_anime_selection_changed();
        let refresh_anime_id = app
            .current_anime_id()
            .expect("second bookmark should exist");
        let refresh_title = app
            .current_anime_title()
            .expect("second bookmark should have a title");
        assert_ne!(refresh_anime_id, aborted_anime_id);
        let (result_tx, _result_rx) = unbounded_channel();
        let (abort_handle, _abort_registration) = futures::future::AbortHandle::new_pair();
        let mut active_metadata_fetch = Some(ActiveMetadataFetch {
            anime_id: Some(aborted_anime_id.clone()),
            target: MetadataTarget::List,
            handle: abort_handle,
        });

        request_metadata_refresh(
            &mut app,
            runtime.handle(),
            &resolver,
            &result_tx,
            &mut active_metadata_fetch,
            AnimeRefreshRequest {
                anime_id: refresh_anime_id.clone(),
                title: refresh_title.clone(),
            },
        );

        let _ = active_metadata_fetch.take();
        app.store_metadata(&refresh_anime_id, &sample_metadata(&refresh_title));

        let retry_candidate = app.next_metadata_fetch_candidate();

        assert_eq!(
            retry_candidate
                .as_ref()
                .map(|candidate| candidate.0.as_str()),
            Some(aborted_anime_id.as_str())
        );
    }
}
