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

use super::{App, EpisodeMarkAction};

impl App {
    pub fn request_play(&mut self) {
        self.request_play_async();
    }

    pub fn request_play_async(&mut self) {
        self.pending_playback_request = true;
    }

    pub fn take_pending_play_async(&mut self) -> bool {
        if self.pending_playback_request {
            self.pending_playback_request = false;
            true
        } else {
            false
        }
    }

    pub fn set_playback_in_progress(&mut self, in_progress: bool) {
        self.playback_in_progress = in_progress;
    }

    pub fn playback_in_progress(&self) -> bool {
        self.playback_in_progress
    }

    pub fn set_current_playing_episode(&mut self, id: Option<String>) {
        self.current_playing_episode_id = id;
        if self.current_playing_episode_id.is_none() {
            self.current_playing_anime_title = None;
            self.current_playing_episode_title = None;
            self.playback_elapsed_seconds = None;
        }
    }

    pub fn current_playing_episode_id(&self) -> Option<&str> {
        self.current_playing_episode_id.as_deref()
    }

    pub fn set_current_playback_titles(&mut self, anime: Option<String>, episode: Option<String>) {
        self.current_playing_anime_title = anime;
        self.current_playing_episode_title = episode;
    }

    pub fn current_playback_label(&self) -> Option<String> {
        match (
            self.current_playing_anime_title.as_deref(),
            self.current_playing_episode_title.as_deref(),
        ) {
            (Some(anime), Some(episode)) => Some(format!("{anime} — {episode}")),
            (Some(anime), None) => Some(anime.to_string()),
            (None, Some(episode)) => Some(episode.to_string()),
            _ => None,
        }
    }

    pub fn set_playback_elapsed(&mut self, elapsed: Option<f64>) {
        self.playback_elapsed_seconds = elapsed;
    }

    pub fn playback_elapsed(&self) -> Option<f64> {
        self.playback_elapsed_seconds
    }

    pub fn request_download(&mut self) {
        if self.current_episode_id().is_none() {
            self.set_details("Highlight an episode to download.");
            return;
        }
        self.pending_download = true;
        if let Some(title) = self.current_episode_title() {
            self.set_details(format!(
                "Preparing download for {title}. Local copies can be removed with 'D'."
            ));
        } else {
            self.set_details("Highlight an episode to download.");
        }
    }

    pub fn take_pending_download(&mut self) -> bool {
        if self.pending_download {
            self.pending_download = false;
            true
        } else {
            false
        }
    }

    pub fn request_delete(&mut self) {
        if self.current_episode_id().is_none() {
            self.set_details("Highlight an episode to delete its download.");
            return;
        }
        self.pending_delete = true;
        if let Some(title) = self.current_episode_title() {
            self.set_details(format!("Preparing to delete local copy of {title}."));
        } else {
            self.set_details("Highlight an episode to delete its download.");
        }
    }

    pub fn take_pending_delete(&mut self) -> bool {
        if self.pending_delete {
            self.pending_delete = false;
            true
        } else {
            false
        }
    }

    pub fn request_bookmark_toggle(&mut self) {
        self.pending_bookmark_toggle = true;
    }

    pub fn take_pending_bookmark_toggle(&mut self) -> bool {
        if self.pending_bookmark_toggle {
            self.pending_bookmark_toggle = false;
            true
        } else {
            false
        }
    }

    pub fn request_mark_current_episode(&mut self, watched: bool) {
        if self.current_episode_id().is_none() {
            self.set_details("Highlight an episode to mark.");
            return;
        }
        self.pending_episode_mark_action = Some(EpisodeMarkAction::Current { watched });
        let verb = if watched { "watched" } else { "unwatched" };
        self.set_details(format!("Marking current episode as {verb}."));
    }

    pub fn request_mark_all_episodes(&mut self, watched: bool) {
        if self.unfiltered_episodes().is_empty() {
            self.set_details("Load episodes to mark them.");
            return;
        }
        self.pending_episode_mark_action = Some(EpisodeMarkAction::All { watched });
        let verb = if watched { "watched" } else { "unwatched" };
        self.set_details(format!("Marking all episodes as {verb}."));
    }

    pub fn request_mark_up_to_current(&mut self) {
        if self.current_episode_id().is_none() {
            self.set_details("Highlight an episode to anchor the range.");
            return;
        }
        if self.unfiltered_episodes().is_empty() {
            self.set_details("Load episodes to mark them.");
            return;
        }
        self.pending_episode_mark_action = Some(EpisodeMarkAction::UpToCurrent);
        self.set_details("Marking episodes up to current as watched.");
    }

    pub fn take_pending_episode_mark_action(&mut self) -> Option<EpisodeMarkAction> {
        self.pending_episode_mark_action.take()
    }
}
