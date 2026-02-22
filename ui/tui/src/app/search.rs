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

use super::{AnimeEntry, App, InputMode, MetaFetch, PendingFlag, SearchModal};

use animestan_core::{AnimeClient, AnimeMetadata, CoreResult, FetchBackend};

impl App {
    pub fn search_query(&self) -> &str {
        &self.search.query
    }

    pub fn search_results_modal_visible(&self) -> bool {
        matches!(self.search.modal_visible, SearchModal::Visible)
    }

    pub fn search_results(&self) -> &[AnimeEntry] {
        &self.data.search_results
    }

    pub fn search_results_query(&self) -> &str {
        &self.search.results_query
    }

    pub fn search_results_selection(&self) -> usize {
        self.search
            .selection
            .min(self.data.search_results.len().saturating_sub(1))
    }

    pub fn current_search_result(&self) -> Option<&AnimeEntry> {
        self.data.search_results.get(self.search.selection)
    }

    pub fn set_search_query<S: Into<String>>(&mut self, query: S) {
        self.search.query = query.into();
    }

    pub fn enter_search_mode(&mut self) {
        self.nav.pending_double_g = false;
        self.nav.input_mode = InputMode::Search;
        if self.visible_bookmark_entries().is_empty() {
            self.nav.left_index = 0;
        } else {
            self.nav.left_index = self
                .nav
                .left_index
                .min(self.visible_bookmark_entries().len().saturating_sub(1));
        }
        self.nav.selected_anime = None;
        self.set_search_query(String::new());
        self.set_details("Search mode: type a query and press Enter.");
    }

    pub fn exit_search_mode(&mut self) {
        self.nav.input_mode = InputMode::Normal;
    }

    pub fn append_search_char(&mut self, ch: char) {
        self.search.query.push(ch);
    }

    pub fn pop_search_char(&mut self) {
        self.search.query.pop();
    }

    pub fn request_search(&mut self) {
        self.search.pending_search = PendingFlag::Yes;
    }

    pub fn take_pending_search(&mut self) -> bool {
        if matches!(self.search.pending_search, PendingFlag::Yes) {
            self.search.pending_search = PendingFlag::No;
            true
        } else {
            false
        }
    }

    pub fn search(&mut self, client: &AnimeClient<FetchBackend>) -> CoreResult<()> {
        let query = self.search.query.trim().to_owned();
        if query.is_empty() {
            self.data.search_results.clear();
            self.search.modal_visible = SearchModal::Hidden;
            self.search.meta_state = MetaFetch::Idle;
            self.clear_episodes();
            self.nav.left_index = 0;
            self.nav.selected_anime = None;
            self.nav.anime_selection_changed = false;
            self.set_details("Enter a search term with 's'.");
            self.refresh_quick_launch_items();
            return Ok(());
        }

        let entries = client.search(&query)?;
        self.data.search_results = entries;
        self.search.results_query.clone_from(&query);
        self.search.selection = 0;
        self.search.modal_visible = if self.data.search_results.is_empty() {
            SearchModal::Hidden
        } else {
            SearchModal::Visible
        };
        self.search.metadata = None;
        self.search.metadata_error = None;
        self.search.meta_state = MetaFetch::Idle;
        self.clear_episodes();
        self.nav.right_index = 0;
        self.nav.selected_episode = None;

        if self.data.search_results.is_empty() {
            self.set_details(format!("No results for '{query}'"));
            self.refresh_quick_launch_items();
            return Ok(());
        }

        self.set_details(format!(
            "Loaded {} search results",
            self.data.search_results.len()
        ));
        self.refresh_quick_launch_items();
        self.request_search_results_metadata();
        Ok(())
    }

    pub fn take_pending_search_results_metadata_fetch(&mut self) -> bool {
        if matches!(self.search.meta_state, MetaFetch::Pending) {
            self.search.meta_state = MetaFetch::Loading;
            true
        } else {
            false
        }
    }

    pub fn next_search_results_metadata_generation(&mut self) -> u64 {
        self.search.metadata_generation = self.search.metadata_generation.wrapping_add(1);
        self.search.metadata_generation
    }

    pub fn current_search_results_metadata_generation(&self) -> u64 {
        self.search.metadata_generation
    }

    pub fn search_results_metadata_loading(&self) -> bool {
        !matches!(self.search.meta_state, MetaFetch::Idle)
    }

    pub fn search_results_metadata(&self) -> Option<&AnimeMetadata> {
        self.search.metadata.as_ref()
    }

    pub fn search_results_metadata_error(&self) -> Option<&str> {
        self.search.metadata_error.as_deref()
    }

    pub fn set_search_results_metadata(&mut self, metadata: AnimeMetadata) {
        self.search.metadata = Some(metadata);
        self.search.meta_state = MetaFetch::Idle;
        self.search.metadata_error = None;
    }

    pub fn set_search_results_metadata_error(&mut self, error: impl Into<String>) {
        self.search.metadata = None;
        self.search.metadata_error = Some(error.into());
        self.search.meta_state = MetaFetch::Idle;
    }

    pub fn close_search_results_modal(&mut self) {
        self.search.modal_visible = SearchModal::Hidden;
        self.search.metadata = None;
        self.search.metadata_error = None;
        self.search.meta_state = MetaFetch::Idle;
    }

    pub fn move_search_results_selection_up(&mut self) {
        if self.data.search_results.is_empty() {
            return;
        }
        let delta = self.search.selection.saturating_sub(1);
        self.set_search_results_selection(delta);
    }

    pub fn move_search_results_selection_down(&mut self) {
        if self.data.search_results.is_empty() {
            return;
        }
        let next =
            (self.search.selection + 1).min(self.data.search_results.len().saturating_sub(1));
        self.set_search_results_selection(next);
    }

    pub(crate) fn search_results_move_to_top(&mut self) {
        self.set_search_results_selection(0);
    }

    pub(crate) fn search_results_move_to_bottom(&mut self) {
        if self.data.search_results.is_empty() {
            return;
        }
        self.set_search_results_selection(self.data.search_results.len() - 1);
    }

    pub(crate) fn search_results_half_page_down(&mut self) {
        let len = self.data.search_results.len();
        if len <= 1 {
            return;
        }
        let step = (len / 2).max(1);
        let target = (self.search.selection + step).min(len - 1);
        self.set_search_results_selection(target);
    }

    pub(crate) fn search_results_half_page_up(&mut self) {
        let len = self.data.search_results.len();
        if len <= 1 {
            return;
        }
        let step = (len / 2).max(1);
        let target = self.search.selection.saturating_sub(step);
        self.set_search_results_selection(target);
    }

    pub fn search_results_selected_title(&self) -> Option<&str> {
        self.current_search_result()
            .map(|entry| entry.title.as_str())
    }

    pub fn request_search_results_add(&mut self) {
        self.search.add_pending = PendingFlag::Yes;
    }

    pub fn take_pending_search_results_add(&mut self) -> bool {
        if matches!(self.search.add_pending, PendingFlag::Yes) {
            self.search.add_pending = PendingFlag::No;
            true
        } else {
            false
        }
    }

    fn request_search_results_metadata(&mut self) {
        if self.data.search_results.is_empty() {
            self.search.metadata = None;
            self.search.metadata_error = None;
            self.search.meta_state = MetaFetch::Idle;
            return;
        }

        self.search.meta_state = MetaFetch::Pending;
        self.search.metadata_error = None;
    }

    fn set_search_results_selection(&mut self, index: usize) {
        if self.data.search_results.is_empty() {
            return;
        }
        let clamped = index.min(self.data.search_results.len().saturating_sub(1));
        if self.search.selection != clamped {
            self.search.selection = clamped;
        }
        self.request_search_results_metadata();
    }
}
