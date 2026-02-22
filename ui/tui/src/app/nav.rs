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

use super::{AnimeEntry, App, Focus, InputMode};

impl App {
    pub fn left_index(&self) -> usize {
        self.left_index
    }

    pub fn right_index(&self) -> usize {
        self.right_index
    }

    pub fn selected_episode(&self) -> Option<usize> {
        self.selected_episode
    }

    pub fn current_anime_title(&self) -> Option<String> {
        self.current_anime_title_ref().map(ToString::to_string)
    }

    pub fn current_anime_id(&self) -> Option<String> {
        self.current_anime().map(|anime| anime.id.clone())
    }

    pub fn current_selection_label(&self) -> String {
        match (self.current_anime_title(), self.current_episode_title()) {
            (Some(anime), Some(episode)) => format!("{anime}-{episode}"),
            (Some(anime), None) => anime,
            (None, Some(episode)) => episode,
            (None, None) => "No Selection".to_string(),
        }
    }

    pub(super) fn current_anime(&self) -> Option<&AnimeEntry> {
        if self.search_results_modal_visible {
            return self.current_search_result();
        }
        self.visible_bookmark_entries()
            .get(self.left_index)
            .map(|entry| &entry.anime)
    }

    fn current_anime_title_ref(&self) -> Option<&str> {
        self.current_anime().map(|anime| anime.title.as_str())
    }

    pub fn current_episode_index(&self) -> Option<usize> {
        let available = self.visible_episodes();
        if available.is_empty() {
            return None;
        }
        let index = self.selected_episode.unwrap_or(self.right_index);
        Some(index.min(available.len() - 1))
    }

    pub fn current_episode_id(&self) -> Option<String> {
        self.current_episode_index().and_then(|index| {
            self.visible_episodes()
                .get(index)
                .map(|episode| episode.id.clone())
        })
    }

    fn current_episode_title_ref(&self) -> Option<&str> {
        self.current_episode_index().and_then(|index| {
            self.visible_episodes()
                .get(index)
                .map(|episode| episode.title.as_str())
        })
    }

    pub fn current_episode_title(&self) -> Option<String> {
        self.current_episode_title_ref().map(ToString::to_string)
    }

    fn left_items_len(&self) -> usize {
        self.visible_bookmark_entries().len()
    }

    fn active_index(&self) -> usize {
        match self.focus {
            Focus::Left => self.left_index,
            Focus::Right => self.right_index,
        }
    }

    fn set_active_index(&mut self, target: usize) {
        match self.focus {
            Focus::Left => {
                let len = self.left_items_len();
                if len == 0 {
                    return;
                }
                let clamped = target.min(len - 1);
                if self.left_index != clamped {
                    self.left_index = clamped;
                    self.anime_selection_changed = true;
                }
            }
            Focus::Right => {
                let len = self.visible_episodes().len();
                if len == 0 {
                    return;
                }
                self.right_index = target.min(len - 1);
            }
        }
    }

    fn active_list_len(&self) -> usize {
        match self.focus {
            Focus::Left => self.left_items_len(),
            Focus::Right => self.visible_episodes().len(),
        }
    }

    pub fn move_up(&mut self) {
        if self.active_list_len() == 0 {
            return;
        }

        match self.focus {
            Focus::Left => {
                let previous = self.left_index;
                if self.left_index > 0 {
                    self.left_index -= 1;
                }
                if self.left_index != previous {
                    self.anime_selection_changed = true;
                }
            }
            Focus::Right => {
                if self.right_index > 0 {
                    self.right_index -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        let len = self.active_list_len();
        if len == 0 {
            return;
        }

        match self.focus {
            Focus::Left => {
                let previous = self.left_index;
                if self.left_index + 1 < len {
                    self.left_index += 1;
                }
                if self.left_index != previous {
                    self.anime_selection_changed = true;
                }
            }
            Focus::Right => {
                if self.right_index + 1 < len {
                    self.right_index += 1;
                }
            }
        }
    }

    pub fn move_to_top(&mut self) {
        let len = self.active_list_len();
        if len <= 1 {
            return;
        }

        self.set_active_index(0);
    }

    pub fn move_to_bottom(&mut self) {
        let len = self.active_list_len();
        if len <= 1 {
            return;
        }

        self.set_active_index(len - 1);
    }

    pub fn move_to_middle(&mut self) {
        let len = self.active_list_len();
        if len <= 1 {
            return;
        }

        self.set_active_index(len / 2);
    }

    pub fn half_page_down(&mut self) {
        let len = self.active_list_len();
        if len <= 1 {
            return;
        }

        let step = (len / 2).max(1);
        let current = self.active_index();
        let target = (current + step).min(len - 1);
        self.set_active_index(target);
    }

    pub fn half_page_up(&mut self) {
        let len = self.active_list_len();
        if len <= 1 {
            return;
        }

        let step = (len / 2).max(1);
        let current = self.active_index();
        let target = current.saturating_sub(step);
        self.set_active_index(target);
    }

    pub(crate) fn start_pending_double_g(&mut self) {
        self.pending_double_g = true;
    }

    pub(crate) fn consume_pending_double_g(&mut self) -> bool {
        if self.pending_double_g {
            self.pending_double_g = false;
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_pending_double_g(&mut self) {
        self.pending_double_g = false;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = self.focus.toggle();
        self.set_details(match self.focus {
            Focus::Left => "Focus: Anime list",
            Focus::Right => "Focus: Episode list",
        });
    }

    pub fn cycle_focus(&mut self) {
        if matches!(self.input_mode, InputMode::Search) {
            self.exit_search_mode();
            self.focus = Focus::Left;
            self.set_details("Focus: Anime list");
            return;
        }

        match self.focus {
            Focus::Left => {
                self.focus = Focus::Right;
                self.set_details("Focus: Episode list");
            }
            Focus::Right => {
                self.enter_search_mode();
            }
        }
    }

    pub fn select_current(&mut self) {
        match self.focus {
            Focus::Left => {
                if self.left_items_len() == 0 {
                    return;
                }
                self.selected_anime = Some(self.left_index);
                if let Some(entry) = self.current_anime() {
                    self.set_details(format!("Selected anime: {}", entry.title));
                }
            }
            Focus::Right => {
                if self.visible_episodes().is_empty() {
                    return;
                }
                self.selected_episode = Some(self.right_index);
                if let Some(episode) = self.visible_episodes().get(self.right_index) {
                    self.set_details(format!("Selected episode: {}", episode.title));
                }
            }
        }
    }

    pub fn take_anime_selection_changed(&mut self) -> bool {
        if self.anime_selection_changed {
            self.anime_selection_changed = false;
            true
        } else {
            false
        }
    }

    pub(super) fn reset_navigation_state(&mut self) {
        self.left_index = 0;
        self.selected_anime = None;
        self.clear_episodes();
        self.pending_double_g = false;
    }
}
