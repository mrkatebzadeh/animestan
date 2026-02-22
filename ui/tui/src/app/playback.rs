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

use super::{App, EpisodeMarkAction, PendingFlag, Progress};

impl App {
    pub fn request_play(&mut self) {
        self.request_play_async();
    }

    pub fn request_play_async(&mut self) {
        self.playback.pending_playback_request = PendingFlag::Yes;
    }

    pub fn take_pending_play_async(&mut self) -> bool {
        if matches!(self.playback.pending_playback_request, PendingFlag::Yes) {
            self.playback.pending_playback_request = PendingFlag::No;
            true
        } else {
            false
        }
    }

    pub fn set_playback_in_progress(&mut self, in_progress: bool) {
        self.playback.in_progress = if in_progress {
            Progress::Active
        } else {
            Progress::Idle
        };
    }

    pub fn playback_in_progress(&self) -> bool {
        matches!(self.playback.in_progress, Progress::Active)
    }

    pub fn set_current_playing_episode(&mut self, id: Option<String>) {
        self.playback.current_episode_id = id;
        if self.playback.current_episode_id.is_none() {
            self.playback.current_anime_title = None;
            self.playback.current_episode_title = None;
            self.playback.elapsed_seconds = None;
        }
    }

    pub fn current_playing_episode_id(&self) -> Option<&str> {
        self.playback.current_episode_id.as_deref()
    }

    pub fn set_current_playback_titles(&mut self, anime: Option<String>, episode: Option<String>) {
        self.playback.current_anime_title = anime;
        self.playback.current_episode_title = episode;
    }

    pub fn current_playback_label(&self) -> Option<String> {
        match (
            self.playback.current_anime_title.as_deref(),
            self.playback.current_episode_title.as_deref(),
        ) {
            (Some(anime), Some(episode)) => Some(format!("{anime} — {episode}")),
            (Some(anime), None) => Some(anime.to_string()),
            (None, Some(episode)) => Some(episode.to_string()),
            _ => None,
        }
    }

    pub fn set_playback_elapsed(&mut self, elapsed: Option<f64>) {
        self.playback.elapsed_seconds = elapsed;
    }

    pub fn playback_elapsed(&self) -> Option<f64> {
        self.playback.elapsed_seconds
    }

    pub fn request_download(&mut self) {
        if self.current_episode_id().is_none() {
            self.set_details("Highlight an episode to download.");
            return;
        }
        self.playback.pending_download = PendingFlag::Yes;
        if let Some(title) = self.current_episode_title() {
            self.set_details(format!(
                "Preparing download for {title}. Local copies can be removed with 'D'."
            ));
        } else {
            self.set_details("Highlight an episode to download.");
        }
    }

    pub fn take_pending_download(&mut self) -> bool {
        if matches!(self.playback.pending_download, PendingFlag::Yes) {
            self.playback.pending_download = PendingFlag::No;
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
        self.playback.pending_delete = PendingFlag::Yes;
        if let Some(title) = self.current_episode_title() {
            self.set_details(format!("Preparing to delete local copy of {title}."));
        } else {
            self.set_details("Highlight an episode to delete its download.");
        }
    }

    pub fn take_pending_delete(&mut self) -> bool {
        if matches!(self.playback.pending_delete, PendingFlag::Yes) {
            self.playback.pending_delete = PendingFlag::No;
            true
        } else {
            false
        }
    }

    pub fn request_bookmark_toggle(&mut self) {
        self.playback.pending_bookmark_toggle = PendingFlag::Yes;
    }

    pub fn take_pending_bookmark_toggle(&mut self) -> bool {
        if matches!(self.playback.pending_bookmark_toggle, PendingFlag::Yes) {
            self.playback.pending_bookmark_toggle = PendingFlag::No;
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
        self.playback.pending_episode_mark_action = Some(EpisodeMarkAction::Current { watched });
        let verb = if watched { "watched" } else { "unwatched" };
        self.set_details(format!("Marking current episode as {verb}."));
    }

    pub fn request_mark_all_episodes(&mut self, watched: bool) {
        if self.unfiltered_episodes().is_empty() {
            self.set_details("Load episodes to mark them.");
            return;
        }
        self.playback.pending_episode_mark_action = Some(EpisodeMarkAction::All { watched });
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
        self.playback.pending_episode_mark_action = Some(EpisodeMarkAction::UpToCurrent);
        self.set_details("Marking episodes up to current as watched.");
    }

    pub fn take_pending_episode_mark_action(&mut self) -> Option<EpisodeMarkAction> {
        self.playback.pending_episode_mark_action.take()
    }
}
