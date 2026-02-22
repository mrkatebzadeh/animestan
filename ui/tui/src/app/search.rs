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

use super::{AnimeEntry, App, InputMode};

use animestan_core::{AnimeClient, AnimeMetadata, CoreResult, FetchBackend};

impl App {
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn search_results_modal_visible(&self) -> bool {
        self.search_results_modal_visible
    }

    pub fn search_results(&self) -> &[AnimeEntry] {
        &self.search_results
    }

    pub fn search_results_query(&self) -> &str {
        &self.search_results_query
    }

    pub fn search_results_selection(&self) -> usize {
        self.search_results_selection
            .min(self.search_results.len().saturating_sub(1))
    }

    pub fn current_search_result(&self) -> Option<&AnimeEntry> {
        self.search_results.get(self.search_results_selection)
    }

    pub fn set_search_query<S: Into<String>>(&mut self, query: S) {
        self.search_query = query.into();
    }

    pub fn enter_search_mode(&mut self) {
        self.pending_double_g = false;
        self.input_mode = InputMode::Search;
        if self.visible_bookmark_entries().is_empty() {
            self.left_index = 0;
        } else {
            self.left_index = self
                .left_index
                .min(self.visible_bookmark_entries().len().saturating_sub(1));
        }
        self.selected_anime = None;
        self.set_search_query(String::new());
        self.set_details("Search mode: type a query and press Enter.");
    }

    pub fn exit_search_mode(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub fn append_search_char(&mut self, ch: char) {
        self.search_query.push(ch);
    }

    pub fn pop_search_char(&mut self) {
        self.search_query.pop();
    }

    pub fn request_search(&mut self) {
        self.pending_search = true;
    }

    pub fn take_pending_search(&mut self) -> bool {
        if self.pending_search {
            self.pending_search = false;
            true
        } else {
            false
        }
    }

    pub fn search(&mut self, client: &AnimeClient<FetchBackend>) -> CoreResult<()> {
        let query = self.search_query.trim().to_owned();
        if query.is_empty() {
            self.search_results.clear();
            self.search_results_modal_visible = false;
            self.clear_episodes();
            self.left_index = 0;
            self.selected_anime = None;
            self.anime_selection_changed = false;
            self.set_details("Enter a search term with 's'.");
            self.refresh_quick_launch_items();
            return Ok(());
        }

        let entries = client.search(&query)?;
        self.search_results = entries;
        self.search_results_query.clone_from(&query);
        self.search_results_selection = 0;
        self.search_results_modal_visible = !self.search_results.is_empty();
        self.search_results_metadata = None;
        self.search_results_metadata_error = None;
        self.search_results_metadata_loading = false;
        self.search_results_metadata_pending = false;
        self.clear_episodes();
        self.right_index = 0;
        self.selected_episode = None;

        if self.search_results.is_empty() {
            self.set_details(format!("No results for '{query}'"));
            self.refresh_quick_launch_items();
            return Ok(());
        }

        self.set_details(format!(
            "Loaded {} search results",
            self.search_results.len()
        ));
        self.refresh_quick_launch_items();
        self.request_search_results_metadata();
        Ok(())
    }

    pub fn take_pending_search_results_metadata_fetch(&mut self) -> bool {
        if self.search_results_metadata_pending {
            self.search_results_metadata_pending = false;
            true
        } else {
            false
        }
    }

    pub fn next_search_results_metadata_generation(&mut self) -> u64 {
        self.search_results_metadata_generation =
            self.search_results_metadata_generation.wrapping_add(1);
        self.search_results_metadata_generation
    }

    pub fn current_search_results_metadata_generation(&self) -> u64 {
        self.search_results_metadata_generation
    }

    pub fn search_results_metadata_loading(&self) -> bool {
        self.search_results_metadata_loading
    }

    pub fn search_results_metadata(&self) -> Option<&AnimeMetadata> {
        self.search_results_metadata.as_ref()
    }

    pub fn search_results_metadata_error(&self) -> Option<&str> {
        self.search_results_metadata_error.as_deref()
    }

    pub fn set_search_results_metadata(&mut self, metadata: AnimeMetadata) {
        self.search_results_metadata = Some(metadata);
        self.search_results_metadata_loading = false;
        self.search_results_metadata_error = None;
    }

    pub fn set_search_results_metadata_error(&mut self, error: impl Into<String>) {
        self.search_results_metadata = None;
        self.search_results_metadata_error = Some(error.into());
        self.search_results_metadata_loading = false;
    }

    pub fn close_search_results_modal(&mut self) {
        self.search_results_modal_visible = false;
        self.search_results_metadata = None;
        self.search_results_metadata_error = None;
        self.search_results_metadata_loading = false;
        self.search_results_metadata_pending = false;
    }

    pub fn move_search_results_selection_up(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        let delta = self.search_results_selection.saturating_sub(1);
        self.set_search_results_selection(delta);
    }

    pub fn move_search_results_selection_down(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        let next = (self.search_results_selection + 1).min(self.search_results.len() - 1);
        self.set_search_results_selection(next);
    }

    pub(crate) fn search_results_move_to_top(&mut self) {
        self.set_search_results_selection(0);
    }

    pub(crate) fn search_results_move_to_bottom(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.set_search_results_selection(self.search_results.len() - 1);
    }

    pub(crate) fn search_results_half_page_down(&mut self) {
        let len = self.search_results.len();
        if len <= 1 {
            return;
        }
        let step = (len / 2).max(1);
        let target = (self.search_results_selection + step).min(len - 1);
        self.set_search_results_selection(target);
    }

    pub(crate) fn search_results_half_page_up(&mut self) {
        let len = self.search_results.len();
        if len <= 1 {
            return;
        }
        let step = (len / 2).max(1);
        let target = self.search_results_selection.saturating_sub(step);
        self.set_search_results_selection(target);
    }

    pub fn search_results_selected_title(&self) -> Option<&str> {
        self.current_search_result()
            .map(|entry| entry.title.as_str())
    }

    pub fn request_search_results_add(&mut self) {
        self.search_results_add_pending = true;
    }

    pub fn take_pending_search_results_add(&mut self) -> bool {
        if self.search_results_add_pending {
            self.search_results_add_pending = false;
            true
        } else {
            false
        }
    }

    fn request_search_results_metadata(&mut self) {
        if self.search_results.is_empty() {
            self.search_results_metadata = None;
            self.search_results_metadata_error = None;
            self.search_results_metadata_loading = false;
            self.search_results_metadata_pending = false;
            return;
        }

        self.search_results_metadata_pending = true;
        self.search_results_metadata_loading = true;
        self.search_results_metadata_error = None;
    }

    fn set_search_results_selection(&mut self, index: usize) {
        if self.search_results.is_empty() {
            return;
        }
        let clamped = index.min(self.search_results.len().saturating_sub(1));
        if self.search_results_selection != clamped {
            self.search_results_selection = clamped;
        }
        self.request_search_results_metadata();
    }
}
