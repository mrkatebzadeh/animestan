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

use animestan_core::{AnimeClient, AppConfig, EpisodeTracker, FetchBackend};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::App;
use crate::cache::{CoverCache, EpisodeCache, cached_episodes};
use crate::flow::{apply_episode_filter, refresh_episode_indicators};
use crate::media::{ImageLoadRequest, queue_image_load};
use crate::tasks::EpisodeFetchRequest;

pub(crate) fn handle_search(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if !app.take_pending_search() {
        return;
    }

    if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
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
