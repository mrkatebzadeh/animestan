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

use super::{
    App, ConfirmExitChoice, DEFAULT_SEARCH_QUERY, FilterMode, Focus, InputMode, Matcher,
    PlaybackStatus,
};
use std::collections::{HashMap, VecDeque};

use crate::events;
use crossterm::event::KeyEvent;
use nucleo::Config;

impl App {
    pub fn new() -> Self {
        Self {
            focus: Focus::Left,
            input_mode: InputMode::Normal,
            left_index: 0,
            right_index: 0,
            selected_anime: None,
            selected_episode: None,
            search_results: Vec::new(),
            bookmark_entries: Vec::new(),
            episodes: Vec::new(),
            filtered_episodes: Vec::new(),
            filtered_bookmark_entries: Vec::new(),
            filtered_episode_entries: Vec::new(),
            panel_filter_mode: false,
            panel_filter_target: None,
            panel_filter_query: String::new(),
            bookmark_filter_active: false,
            episode_filter_active: false,
            bookmark_filter_query: String::new(),
            episode_filter_query: String::new(),
            episodes_loading: false,
            fetch_generation: 0,
            search_query: DEFAULT_SEARCH_QUERY.to_string(),
            pending_search: false,
            pending_playback_request: false,
            pending_download: false,
            pending_delete: false,
            pending_bookmark_toggle: false,
            pending_episode_mark_action: None,
            pending_double_g: false,
            anime_selection_changed: false,
            filter_changed: false,
            filter_mode: FilterMode::None,
            playback_status: PlaybackStatus::None,
            playback_in_progress: false,
            current_playing_episode_id: None,
            current_playing_anime_title: None,
            current_playing_episode_title: None,
            playback_elapsed_seconds: None,
            details_text: concat!(
                "Press s to search, / to filter panels, w/u to mark current episodes, ",
                "W/U to mark all, K to mark through current, f for filters, ",
                "Space to select, d to download, D to delete, q to quit, Ctrl+M to mark search results."
            )
            .to_string(),
            should_quit: false,
            confirm_exit: false,
            confirm_exit_choice: ConfirmExitChoice::Yes,
            show_keybindings: false,
            matcher: Matcher::new(Config::DEFAULT),
            episode_indicators: HashMap::new(),
            anime_progress: HashMap::new(),
            quick_launch_active: false,
            quick_launch_query: String::new(),
            quick_launch_selection: 0,
            quick_launch_items: Vec::new(),
            quick_launch_history: VecDeque::new(),
            quick_launch_recently_played: VecDeque::new(),
            last_played_episode: None,
            pending_playback_override: None,
            info_modal_visible: false,
            info_modal_loading: false,
            info_modal_metadata: None,
            info_modal_error: None,
            pending_info_fetch: false,
            info_fetch_generation: 0,
            search_results_query: String::new(),
            search_results_modal_visible: false,
            search_results_selection: 0,
            search_results_metadata: None,
            search_results_metadata_error: None,
            search_results_metadata_loading: false,
            search_results_metadata_generation: 0,
            search_results_metadata_pending: false,
            search_results_add_pending: false,
            keybindings_scroll: 0,
            keybindings_content_lines: 0,
            keybindings_viewport_lines: 0,
        }
    }

    pub fn on_key(&mut self, key_event: KeyEvent) {
        events::handle_key_event(self, key_event);
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    pub fn mode_label(&self) -> &'static str {
        if matches!(self.input_mode, InputMode::Search) || self.panel_filter_mode {
            "Insert"
        } else {
            "Normal"
        }
    }

    pub fn set_playback_status(&mut self, status: PlaybackStatus) {
        self.playback_status = status;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn details(&self) -> &str {
        &self.details_text
    }

    pub fn set_details<S: Into<String>>(&mut self, details: S) {
        self.details_text = details.into();
    }

    pub fn show_help(&mut self) {
        self.toggle_keybindings();
    }

    pub fn request_exit(&mut self) {
        self.confirm_exit = true;
        self.confirm_exit_choice = ConfirmExitChoice::Yes;
    }

    pub fn confirm_exit(&self) -> bool {
        self.confirm_exit
    }

    pub fn clear_confirm_exit(&mut self) {
        self.confirm_exit = false;
        self.confirm_exit_choice = ConfirmExitChoice::Yes;
    }

    pub fn confirm_exit_and_quit(&mut self) {
        self.should_quit = true;
        self.confirm_exit = false;
        self.confirm_exit_choice = ConfirmExitChoice::Yes;
    }

    pub fn confirm_exit_choice(&self) -> ConfirmExitChoice {
        self.confirm_exit_choice
    }

    pub fn set_confirm_exit_choice(&mut self, choice: ConfirmExitChoice) {
        self.confirm_exit_choice = choice;
    }

    pub fn toggle_confirm_exit_choice(&mut self) {
        self.confirm_exit_choice = self.confirm_exit_choice.toggle();
    }

    pub fn toggle_keybindings(&mut self) {
        self.show_keybindings = !self.show_keybindings;
        if self.show_keybindings {
            self.keybindings_scroll = 0;
            self.keybindings_content_lines = 0;
            self.keybindings_viewport_lines = 0;
        }
    }

    pub fn show_keybindings(&self) -> bool {
        self.show_keybindings
    }

    pub fn keybindings_scroll(&self) -> usize {
        self.keybindings_scroll
    }

    pub fn keybindings_viewport_lines(&self) -> usize {
        self.keybindings_viewport_lines
    }

    pub fn set_keybindings_content_lines(&mut self, lines: usize) {
        self.keybindings_content_lines = lines;
        self.clamp_keybindings_scroll();
    }

    pub fn set_keybindings_viewport_lines(&mut self, lines: usize) {
        self.keybindings_viewport_lines = lines;
        self.clamp_keybindings_scroll();
    }

    pub fn scroll_keybindings(&mut self, delta: i64) {
        if self.keybindings_viewport_lines == 0
            || self.keybindings_content_lines <= self.keybindings_viewport_lines
        {
            self.keybindings_scroll = 0;
            return;
        }

        let max = self.max_keybindings_scroll();
        let current = i64::try_from(self.keybindings_scroll).unwrap_or(0);
        let max_scroll = i64::try_from(max).unwrap_or(i64::MAX);
        let target = (current + delta).clamp(0, max_scroll);
        self.keybindings_scroll = usize::try_from(target).unwrap_or(max);
    }

    pub fn set_keybindings_scroll(&mut self, offset: usize) {
        self.keybindings_scroll = offset.min(self.max_keybindings_scroll());
    }

    pub fn keybindings_max_scroll(&self) -> usize {
        self.max_keybindings_scroll()
    }

    fn clamp_keybindings_scroll(&mut self) {
        let max_scroll = self.max_keybindings_scroll();
        if self.keybindings_scroll > max_scroll {
            self.keybindings_scroll = max_scroll;
        }
    }

    fn max_keybindings_scroll(&self) -> usize {
        self.keybindings_content_lines
            .saturating_sub(self.keybindings_viewport_lines)
    }
}
