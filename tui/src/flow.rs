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
use std::sync::{Arc, Mutex};

use animestan_core::{
    AnimeClient, AppConfig, EpisodeTracker, FetchBackend, episode_file_path, local_playback_url,
};
use anyhow::{Result, anyhow};
use futures::future::AbortHandle;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use crate::app::{App, EpisodeIndicators, PlaybackStatus};
use crate::cache::{EpisodeCache, cache_episodes};
use crate::tasks::{
    EpisodeFetchRequest, EpisodeFetchResult, PlaybackRequest, PlaybackResult,
    spawn_episode_fetch_task, spawn_playback_task,
};

pub(crate) fn drain_episode_fetch_requests(
    app: &mut App,
    runtime: &Handle,
    client: &Arc<AnimeClient<FetchBackend>>,
    request_rx: &mut UnboundedReceiver<EpisodeFetchRequest>,
    result_tx: &UnboundedSender<EpisodeFetchResult>,
    active_fetch: &mut Option<AbortHandle>,
) {
    loop {
        match request_rx.try_recv() {
            Ok(request) => {
                if let Some(handle) = active_fetch.take() {
                    handle.abort();
                }
                let abort_handle = spawn_episode_fetch_task(
                    runtime,
                    Arc::clone(client),
                    request,
                    result_tx.clone(),
                );
                *active_fetch = Some(abort_handle);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                app.set_details("Episode fetch queue disconnected.");
                app.set_episodes_loading(false);
                break;
            }
        }
    }
}

pub(crate) fn drain_episode_fetch_results(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
    episode_cache: &Arc<Mutex<EpisodeCache>>,
    result_rx: &mut UnboundedReceiver<EpisodeFetchResult>,
) {
    loop {
        match result_rx.try_recv() {
            Ok(fetch_result) => {
                if fetch_result.generation != app.current_fetch_generation() {
                    continue;
                }

                match fetch_result.result {
                    Ok(episodes) => {
                        let count = episodes.len();
                        let anime_id = fetch_result.anime_id.clone();
                        app.set_episodes(episodes.clone());
                        if let Err(err) = cache_episodes(episode_cache, &anime_id, &episodes) {
                            app.set_details(format!("Failed to cache episodes: {err}"));
                        }
                        app.set_details(format!("Loaded {count} episodes"));
                        if app.current_filter().is_some() {
                            if let Err(err) = apply_episode_filter(app, tracker) {
                                app.set_details(format!("Filter failed: {err}"));
                            }
                        }
                        if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                            app.set_details(format!("Failed to refresh indicators: {err}"));
                        }
                        app.clear_episode_refresh_pending(&anime_id);
                    }
                    Err(err) => {
                        app.set_episodes_loading(false);
                        app.set_details(format!("Episode load failed: {err}"));
                        app.clear_episode_refresh_pending(&fetch_result.anime_id);
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                app.set_details("Episode fetch worker disconnected.");
                app.set_episodes_loading(false);
                break;
            }
        }
    }
}

pub(crate) fn handle_playback_requests(
    app: &mut App,
    config: &Arc<AppConfig>,
    playback_request_tx: &UnboundedSender<PlaybackRequest>,
) {
    if !app.take_pending_play_async() {
        return;
    }

    if app.playback_in_progress() {
        app.set_details("Playback already running");
        return;
    }

    let (episode_id, episode_title, anime_id) =
        if let Some((episode_id, episode_title, anime_id)) = app.take_pending_playback_override() {
            (episode_id, episode_title, anime_id)
        } else {
            let Some(episode_id) = app.current_episode_id() else {
                app.set_details("Highlight an episode to play");
                return;
            };
            let anime_id = app.current_anime_id();
            let episode_title = app.current_episode_title();
            (episode_id, episode_title, anime_id)
        };
    let using_local = local_playback_url(config, &episode_id).is_some();

    if let Some(title) = &episode_title {
        if using_local {
            app.set_details(format!("Launching local playback for {title}"));
        } else {
            app.set_details(format!("Launching player for {title}"));
        }
    } else if using_local {
        app.set_details("Launching local playback...");
    } else {
        app.set_details("Launching player...");
    }

    let request = PlaybackRequest {
        episode_id: episode_id.clone(),
        episode_title: episode_title.clone(),
    };
    let requested_title = request.episode_title.clone();

    if playback_request_tx.send(request).is_err() {
        app.set_details("Playback queue disconnected.");
        app.set_current_playing_episode(None);
        return;
    }

    app.record_played_episode(episode_id.clone(), anime_id, requested_title);
    app.set_current_playback_titles(app.current_anime_title(), episode_title.clone());
    app.set_current_playing_episode(Some(episode_id));
    app.set_playback_in_progress(true);
    app.set_playback_status(PlaybackStatus::Playing);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn drain_playback_request_queue(
    app: &mut App,
    runtime: &Handle,
    client: &Arc<AnimeClient<FetchBackend>>,
    config: &Arc<AppConfig>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    request_rx: &mut UnboundedReceiver<PlaybackRequest>,
    result_tx: &UnboundedSender<PlaybackResult>,
    active_playback: &mut Option<AbortHandle>,
) {
    loop {
        match request_rx.try_recv() {
            Ok(request) => {
                if let Some(handle) = active_playback.take() {
                    handle.abort();
                }
                let abort_handle = spawn_playback_task(
                    runtime,
                    Arc::clone(client),
                    Arc::clone(config),
                    Arc::clone(tracker),
                    request,
                    result_tx.clone(),
                );
                *active_playback = Some(abort_handle);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                app.set_details("Playback request queue disconnected.");
                app.set_playback_status(PlaybackStatus::None);
                app.set_playback_in_progress(false);
                app.set_current_playing_episode(None);
                break;
            }
        }
    }
}

pub(crate) fn drain_playback_results(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &Arc<AppConfig>,
    result_rx: &mut UnboundedReceiver<PlaybackResult>,
    active_playback: &mut Option<AbortHandle>,
) {
    loop {
        match result_rx.try_recv() {
            Ok(result) => {
                *active_playback = None;
                app.set_playback_status(PlaybackStatus::None);
                app.set_playback_in_progress(false);
                app.set_current_playing_episode(None);

                let PlaybackResult {
                    episode_title,
                    outcome,
                } = result;

                match outcome {
                    Ok(()) => {
                        if let Some(title) = episode_title {
                            app.set_details(format!("Finished playing {title}"));
                        } else {
                            app.set_details("Playback finished");
                        }
                    }
                    Err(err) => {
                        app.set_details(format!("Playback failed: {err}"));
                    }
                }

                if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                    app.set_details(format!("Failed to refresh indicators: {err}"));
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                app.set_details("Playback worker disconnected.");
                app.set_playback_status(PlaybackStatus::None);
                app.set_playback_in_progress(false);
                app.set_current_playing_episode(None);
                break;
            }
        }
    }
}

pub(crate) fn apply_episode_filter(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
) -> Result<()> {
    if let Some(filter) = app.current_filter() {
        let filtered = {
            let guard = tracker
                .lock()
                .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
            guard.filter_episodes(app.unfiltered_episodes(), filter)
        };
        app.set_filtered_episodes(filtered);
    } else {
        app.clear_filtered_episodes();
    }

    Ok(())
}

pub(crate) fn refresh_episode_indicators(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
) -> Result<()> {
    let indicators = {
        let guard = tracker
            .lock()
            .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
        let mut indicators = HashMap::with_capacity(app.unfiltered_episodes().len());
        for episode in app.unfiltered_episodes() {
            let state = guard.state_for(&episode.id);
            let watched = state.as_ref().is_some_and(|status| status.watched);
            let in_progress = state.as_ref().is_some_and(|status| status.in_progress);
            let downloaded = episode_file_path(config, &episode.id).exists();
            indicators.insert(
                episode.id.clone(),
                EpisodeIndicators {
                    watched,
                    in_progress,
                    downloaded,
                },
            );
        }
        indicators
    };
    app.set_episode_indicators(indicators);
    app.record_selected_anime_progress();
    Ok(())
}

pub(crate) fn update_playback_elapsed(app: &mut App, tracker: &Arc<Mutex<EpisodeTracker>>) {
    if let Some(episode_id) = app.current_playing_episode_id() {
        if let Ok(guard) = tracker.lock() {
            let elapsed = guard
                .progress_for(episode_id)
                .and_then(|progress| progress.last_position_sec);
            app.set_playback_elapsed(elapsed);
        } else {
            app.set_playback_elapsed(None);
        }
    } else {
        app.set_playback_elapsed(None);
    }
}
