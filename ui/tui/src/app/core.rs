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
    App, ConfirmExitChoice, DEFAULT_SEARCH_QUERY, DataState, FilterState, Focus, InputMode,
    KeybindingsState, Matcher, ModalState, NavState, PanelMode, PlaybackState, PlaybackStatus,
    QuickLaunchState, SearchState, UiState,
};

use crate::events;
use crossterm::event::KeyEvent;
use nucleo::Config;

impl App {
    pub fn new() -> Self {
        Self {
            nav: NavState {
                focus: Focus::Left,
                input_mode: InputMode::Normal,
                left_index: 0,
                right_index: 0,
                selected_anime: None,
                selected_episode: None,
                pending_double_g: false,
                anime_selection_changed: false,
            },
            filters: FilterState::default(),
            data: DataState::default(),
            search: SearchState {
                query: DEFAULT_SEARCH_QUERY.to_string(),
                ..Default::default()
            },
            playback: PlaybackState::default(),
            quick: QuickLaunchState::default(),
            modal: ModalState::default(),
            keybindings: KeybindingsState::default(),
            ui: UiState {
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
            },
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub fn on_key(&mut self, key_event: KeyEvent) {
        events::handle_key_event(self, key_event);
    }

    pub fn focus(&self) -> Focus {
        self.nav.focus
    }

    pub fn input_mode(&self) -> InputMode {
        self.nav.input_mode
    }

    pub fn mode_label(&self) -> &'static str {
        if matches!(self.nav.input_mode, InputMode::Search)
            || matches!(self.filters.panel_mode, PanelMode::Active)
        {
            "Insert"
        } else {
            "Normal"
        }
    }

    pub fn set_playback_status(&mut self, status: PlaybackStatus) {
        self.playback.status = status;
    }

    pub fn should_quit(&self) -> bool {
        self.ui.should_quit
    }

    pub fn details(&self) -> &str {
        &self.ui.details_text
    }

    pub fn set_details<S: Into<String>>(&mut self, details: S) {
        self.ui.details_text = details.into();
    }

    pub fn show_help(&mut self) {
        self.toggle_keybindings();
    }

    pub fn request_exit(&mut self) {
        self.ui.confirm_exit = true;
        self.ui.confirm_exit_choice = ConfirmExitChoice::Yes;
    }

    pub fn confirm_exit(&self) -> bool {
        self.ui.confirm_exit
    }

    pub fn clear_confirm_exit(&mut self) {
        self.ui.confirm_exit = false;
        self.ui.confirm_exit_choice = ConfirmExitChoice::Yes;
    }

    pub fn confirm_exit_and_quit(&mut self) {
        self.ui.should_quit = true;
        self.ui.confirm_exit = false;
        self.ui.confirm_exit_choice = ConfirmExitChoice::Yes;
    }

    pub fn confirm_exit_choice(&self) -> ConfirmExitChoice {
        self.ui.confirm_exit_choice
    }

    pub fn set_confirm_exit_choice(&mut self, choice: ConfirmExitChoice) {
        self.ui.confirm_exit_choice = choice;
    }

    pub fn toggle_confirm_exit_choice(&mut self) {
        self.ui.confirm_exit_choice = self.ui.confirm_exit_choice.toggle();
    }

    pub fn toggle_keybindings(&mut self) {
        self.ui.show_keybindings = !self.ui.show_keybindings;
        if self.ui.show_keybindings {
            self.keybindings.scroll = 0;
            self.keybindings.content_lines = 0;
            self.keybindings.viewport_lines = 0;
        }
    }

    pub fn show_keybindings(&self) -> bool {
        self.ui.show_keybindings
    }

    pub fn keybindings_scroll(&self) -> usize {
        self.keybindings.scroll
    }

    pub fn keybindings_viewport_lines(&self) -> usize {
        self.keybindings.viewport_lines
    }

    pub fn set_keybindings_content_lines(&mut self, lines: usize) {
        self.keybindings.content_lines = lines;
        self.clamp_keybindings_scroll();
    }

    pub fn set_keybindings_viewport_lines(&mut self, lines: usize) {
        self.keybindings.viewport_lines = lines;
        self.clamp_keybindings_scroll();
    }

    pub fn scroll_keybindings(&mut self, delta: i64) {
        if self.keybindings.viewport_lines == 0
            || self.keybindings.content_lines <= self.keybindings.viewport_lines
        {
            self.keybindings.scroll = 0;
            return;
        }

        let max = self.max_keybindings_scroll();
        let current = i64::try_from(self.keybindings.scroll).unwrap_or(0);
        let max_scroll = i64::try_from(max).unwrap_or(i64::MAX);
        let target = (current + delta).clamp(0, max_scroll);
        self.keybindings.scroll = usize::try_from(target).unwrap_or(max);
    }

    pub fn set_keybindings_scroll(&mut self, offset: usize) {
        self.keybindings.scroll = offset.min(self.max_keybindings_scroll());
    }

    pub fn keybindings_max_scroll(&self) -> usize {
        self.max_keybindings_scroll()
    }

    fn clamp_keybindings_scroll(&mut self) {
        let max_scroll = self.max_keybindings_scroll();
        if self.keybindings.scroll > max_scroll {
            self.keybindings.scroll = max_scroll;
        }
    }

    fn max_keybindings_scroll(&self) -> usize {
        self.keybindings
            .content_lines
            .saturating_sub(self.keybindings.viewport_lines)
    }
}
