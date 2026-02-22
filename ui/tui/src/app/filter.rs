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
    App, FilterActive, FilterCandidate, FilterTarget, Focus, Matcher, PanelMode, PendingFlag,
    PlaybackFilter,
};

use nucleo::pattern::{CaseMatching, Normalization, Pattern};

impl App {
    pub fn panel_filter_mode(&self) -> bool {
        matches!(self.filters.panel_mode, PanelMode::Active)
    }

    pub fn panel_filter_target(&self) -> Option<FilterTarget> {
        self.filters.panel_target
    }

    pub fn panel_filter_query(&self) -> &str {
        &self.filters.panel_query
    }

    pub fn panel_filter_active_for(&self, target: FilterTarget) -> bool {
        match target {
            FilterTarget::Anime | FilterTarget::Bookmarks => {
                matches!(self.filters.bookmark_active, FilterActive::Active)
            }
            FilterTarget::Episodes => matches!(self.filters.episode_active, FilterActive::Active),
        }
    }

    pub fn filter_target_for_focus(&self) -> FilterTarget {
        match self.nav.focus {
            Focus::Left => FilterTarget::Anime,
            Focus::Right => FilterTarget::Episodes,
        }
    }

    pub fn enter_panel_filter(&mut self, target: FilterTarget) {
        self.nav.pending_double_g = false;
        self.filters.panel_mode = PanelMode::Active;
        self.filters.panel_target = Some(target);
        self.filters.panel_query.clear();
        self.set_details("Panel filter: type to narrow results, Enter to apply, Esc to close.");
    }

    pub fn exit_panel_filter(&mut self) {
        self.filters.panel_mode = PanelMode::Inactive;
        self.filters.panel_target = None;
    }

    pub fn update_panel_filter_query(&mut self, query: String) {
        self.filters.panel_query = query;
        if let Some(target) = self.filters.panel_target {
            self.apply_panel_filter_for_target(target);
        }
    }

    pub fn filter_label(&self) -> Option<&'static str> {
        self.filters.filter_mode.label()
    }

    pub fn current_filter(&self) -> Option<PlaybackFilter> {
        self.filters.filter_mode.as_filter()
    }

    pub fn take_filter_changed(&mut self) -> bool {
        if matches!(self.filters.filter_changed, PendingFlag::Yes) {
            self.filters.filter_changed = PendingFlag::No;
            true
        } else {
            false
        }
    }

    pub fn cycle_filter(&mut self) {
        self.filters.filter_mode = self.filters.filter_mode.next();
        self.filters.filter_changed = PendingFlag::Yes;
        self.nav.right_index = 0;
        self.nav.selected_episode = None;
        if let Some(label) = self.filters.filter_mode.label() {
            self.set_details(format!("Filter set to {label}"));
            self.data.filtered_episodes.clear();
        } else {
            self.set_details("Filters cleared");
            self.clear_filtered_episodes();
        }
    }

    fn apply_panel_filter_for_target(&mut self, target: FilterTarget) {
        let query = self.filters.panel_query.clone();
        self.apply_panel_filter_with_query(target, &query);
        *self.saved_query_mut(target) = query;
    }

    fn apply_panel_filter_with_query(&mut self, target: FilterTarget, query: &str) {
        let trimmed = query.trim();
        match target {
            FilterTarget::Anime | FilterTarget::Bookmarks => {
                self.apply_bookmark_filter(trimmed);
            }
            FilterTarget::Episodes => {
                self.apply_episode_filter(trimmed);
            }
        }
    }

    fn apply_bookmark_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.filters.bookmark_active = FilterActive::Inactive;
            self.data.filtered_bookmark_entries.clear();
            self.nav.left_index = self
                .nav
                .left_index
                .min(self.visible_bookmark_entries().len().saturating_sub(1));
            self.nav.selected_anime = None;
            self.nav.anime_selection_changed = true;
            return;
        }

        let filtered = fuzzy_filter(
            &mut self.matcher,
            &self.data.bookmark_entries,
            query,
            |entry| entry.anime.title.as_str(),
        );
        self.data.filtered_bookmark_entries = filtered;
        self.filters.bookmark_active = FilterActive::Active;
        if self.nav.left_index >= self.visible_bookmark_entries().len() {
            self.nav.left_index = 0;
            self.nav.selected_anime = None;
        }
        self.nav.anime_selection_changed = true;
    }

    fn apply_episode_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.filters.episode_active = FilterActive::Inactive;
            self.data.filtered_episode_entries.clear();
            self.nav.right_index = self
                .nav
                .right_index
                .min(self.visible_episodes().len().saturating_sub(1));
            self.nav.selected_episode = None;
            return;
        }

        let base = self.base_episode_entries().to_vec();
        let filtered = fuzzy_filter(&mut self.matcher, &base, query, |episode| {
            episode.title.as_str()
        });
        self.data.filtered_episode_entries = filtered;
        self.filters.episode_active = FilterActive::Active;
        if self.nav.right_index >= self.visible_episodes().len() {
            self.nav.right_index = 0;
            self.nav.selected_episode = None;
        }
    }

    fn saved_query_mut(&mut self, target: FilterTarget) -> &mut String {
        match target {
            FilterTarget::Anime | FilterTarget::Bookmarks => &mut self.filters.bookmark_query,
            FilterTarget::Episodes => &mut self.filters.episode_query,
        }
    }

    fn saved_query(&self, target: FilterTarget) -> &str {
        match target {
            FilterTarget::Anime | FilterTarget::Bookmarks => &self.filters.bookmark_query,
            FilterTarget::Episodes => &self.filters.episode_query,
        }
    }

    pub(super) fn apply_saved_panel_filter(&mut self, target: FilterTarget) {
        if !self.panel_filter_active_for(target) {
            return;
        }

        let saved = self.saved_query(target).to_string();
        if saved.trim().is_empty() {
            return;
        }
        self.apply_panel_filter_with_query(target, &saved);
    }
}

fn fuzzy_filter<T, F>(matcher: &mut Matcher, source: &[T], query: &str, title_fn: F) -> Vec<T>
where
    T: Clone,
    F: Fn(&T) -> &str,
{
    if source.is_empty() {
        return Vec::new();
    }

    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let candidates: Vec<_> = source
        .iter()
        .enumerate()
        .map(|(index, item)| FilterCandidate {
            index,
            title: title_fn(item),
        })
        .collect();

    let mut ranked = pattern.match_list(candidates, matcher);
    ranked.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.index.cmp(&right.index))
    });

    ranked
        .into_iter()
        .map(|(candidate, _)| source[candidate.index].clone())
        .collect()
}
