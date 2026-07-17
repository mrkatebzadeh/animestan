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
    AnimeClient, AppConfig, Episode, EpisodeTracker, FetchBackend, delete_episode,
    download_episode, episode_file_path, local_playback_url,
};
use spdlog::prelude::*;

use crate::app::{App, EpisodeMarkAction, PlaybackStatus};
use crate::flow::refresh_episode_indicators;

pub(crate) fn handle_download(
    app: &mut App,
    config: &AppConfig,
    client: &AnimeClient<FetchBackend>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
) -> bool {
    if !app.take_pending_download() {
        return false;
    }

    let Some(episode_id) = app.current_episode_id() else {
        app.set_details("Highlight an episode to download.");
        return true;
    };

    info!("download requested from TUI for '{episode_id}'");
    let episode_title = app.current_episode_title();

    if local_playback_url(config, &episode_id).is_some() {
        let path = episode_file_path(config, &episode_id);
        app.set_details(format!("Episode already downloaded at {}", path.display()));
        info!(
            "episode '{episode_id}' already downloaded locally at {}",
            path.display()
        );
        if let Err(err) = refresh_episode_indicators(app, tracker, config) {
            app.set_details(format!("Failed to refresh indicators: {err}"));
        }
        return true;
    }

    app.set_playback_status(PlaybackStatus::Downloading);
    let stream = match client.resolve_stream_url(&episode_id) {
        Ok(link) => link,
        Err(err) => {
            app.set_playback_status(PlaybackStatus::None);
            app.set_details(format!("Failed to resolve stream: {err}"));
            return true;
        }
    };

    let download_result = download_episode(config, &episode_id, &stream.url);
    app.set_playback_status(PlaybackStatus::None);

    match download_result {
        Ok(saved_path) => {
            info!(
                "downloaded episode '{episode_id}' to {}",
                saved_path.display()
            );
            if let Some(title) = episode_title {
                app.set_details(format!("Downloaded {title} to {}", saved_path.display()));
            } else {
                app.set_details(format!("Download saved to {}", saved_path.display()));
            }
            if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                app.set_details(format!("Failed to refresh indicators: {err}"));
            }
        }
        Err(err) => {
            app.set_details(format!("Download failed: {err}"));
        }
    }

    false
}

pub(crate) fn handle_delete(
    app: &mut App,
    config: &AppConfig,
    tracker: &Arc<Mutex<EpisodeTracker>>,
) -> bool {
    if !app.take_pending_delete() {
        return false;
    }

    let Some(episode_id) = app.current_episode_id() else {
        app.set_details("Highlight an episode to delete its download.");
        return true;
    };

    info!("delete requested from TUI for '{episode_id}'");
    let episode_title = app.current_episode_title();
    match delete_episode(config, &episode_id) {
        Ok(true) => {
            info!("deleted download for '{episode_id}'");
            if let Some(title) = episode_title {
                app.set_details(format!("Deleted download for {title}"));
            } else {
                app.set_details("Deleted download.");
            }
            if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                app.set_details(format!("Failed to refresh indicators: {err}"));
            }
        }
        Ok(false) => {
            info!("no download found for '{episode_id}'");
            app.set_details("No download found to delete.");
        }
        Err(err) => {
            app.set_details(format!("Delete failed: {err}"));
        }
    }

    false
}

pub(crate) fn handle_episode_mark_actions(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
) {
    let Some(action) = app.take_pending_episode_mark_action() else {
        return;
    };

    let mark_result = {
        let Ok(mut guard) = tracker.lock() else {
            app.set_details("Episode tracker lock poisoned.");
            return;
        };
        perform_episode_mark_action(app, &mut guard, action)
    };

    let message = match mark_result {
        Ok(message) => message,
        Err(err) => {
            app.set_details(err);
            return;
        }
    };

    if let Err(err) = refresh_episode_indicators(app, tracker, config) {
        app.set_details(format!("Failed to refresh indicators: {err}"));
    } else {
        app.set_details(message);
    }
}

fn perform_episode_mark_action(
    app: &App,
    tracker: &mut EpisodeTracker,
    action: EpisodeMarkAction,
) -> Result<String, String> {
    match action {
        EpisodeMarkAction::Current { watched } => {
            let episode_id = app
                .current_episode_id()
                .ok_or_else(|| "Highlight an episode to mark it.".to_string())?;
            let result = if watched {
                tracker.mark_watched(&episode_id)
            } else {
                tracker.mark_unwatched(&episode_id)
            };
            result.map_err(|err| err.to_string())?;
            let message = if watched {
                "Marked current episode as watched."
            } else {
                "Marked current episode as unwatched."
            };
            Ok(message.to_string())
        }
        EpisodeMarkAction::All { watched } => {
            let episodes = app.unfiltered_episodes();
            if episodes.is_empty() {
                return Err("No episodes loaded to mark.".to_string());
            }
            let ids: Vec<String> = episodes.iter().map(|episode| episode.id.clone()).collect();
            tracker
                .mark_many(&ids, watched)
                .map_err(|err| err.to_string())?;
            let message = if watched {
                "Marked all loaded episodes as watched."
            } else {
                "Marked all loaded episodes as unwatched."
            };
            Ok(message.to_string())
        }
        EpisodeMarkAction::UpToCurrent => {
            let current_id = app
                .current_episode_id()
                .ok_or_else(|| "Highlight an episode to set the range.".to_string())?;
            let mut episodes: Vec<Episode> = app.unfiltered_episodes().to_vec();
            if episodes.is_empty() {
                return Err("No episodes loaded to mark.".to_string());
            }
            episodes.sort_by_key(|episode| episode.number);
            let mut ids = Vec::new();
            for episode in episodes {
                ids.push(episode.id.clone());
                if episode.id == current_id {
                    break;
                }
            }
            if ids.is_empty() || ids.last() != Some(&current_id) {
                return Err("Current episode is not present in the loaded list.".to_string());
            }
            tracker
                .mark_many(&ids, true)
                .map_err(|err| err.to_string())?;
            Ok("Marked episodes up to current as watched.".to_string())
        }
    }
}
