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

use animestan_core::{
    AnimeClient, AnimeEntry, CoreResult, Episode, FavoriteEntry, FavoriteStore, FetchBackend,
    PlaybackFilter,
};
use crossterm::event::KeyEvent;
use nucleo::{
    Config, Matcher,
    pattern::{CaseMatching, Normalization, Pattern},
};

use crate::events;

const DEFAULT_SEARCH_QUERY: &str = "naruto";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
    Left,
    Right,
}

impl Focus {
    fn toggle(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FilterTarget {
    Anime,
    Episodes,
    Bookmarks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeftPaneMode {
    Search,
    Bookmarks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterMode {
    None,
    Unwatched,
    InProgress,
    Next,
    Recent,
}

impl FilterMode {
    const fn label(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Unwatched => Some("Unwatched"),
            Self::InProgress => Some("In Progress"),
            Self::Next => Some("Next"),
            Self::Recent => Some("Recent"),
        }
    }

    const fn as_filter(self) -> Option<PlaybackFilter> {
        match self {
            Self::None => None,
            Self::Unwatched => Some(PlaybackFilter::Unwatched),
            Self::InProgress => Some(PlaybackFilter::InProgress),
            Self::Next => Some(PlaybackFilter::Next),
            Self::Recent => Some(PlaybackFilter::Recent),
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::None => Self::Unwatched,
            Self::Unwatched => Self::InProgress,
            Self::InProgress => Self::Next,
            Self::Next => Self::Recent,
            Self::Recent => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EpisodeIndicators {
    pub watched: bool,
    pub in_progress: bool,
    pub downloaded: bool,
}

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    focus: Focus,
    input_mode: InputMode,
    left_index: usize,
    right_index: usize,
    selected_anime: Option<usize>,
    selected_episode: Option<usize>,
    anime_entries: Vec<AnimeEntry>,
    bookmark_entries: Vec<FavoriteEntry>,
    episodes: Vec<Episode>,
    filtered_episodes: Vec<Episode>,
    filtered_anime_entries: Vec<AnimeEntry>,
    filtered_bookmark_entries: Vec<FavoriteEntry>,
    filtered_episode_entries: Vec<Episode>,
    panel_filter_mode: bool,
    panel_filter_target: Option<FilterTarget>,
    panel_filter_query: String,
    anime_filter_active: bool,
    bookmark_filter_active: bool,
    episode_filter_active: bool,
    anime_filter_query: String,
    bookmark_filter_query: String,
    episode_filter_query: String,
    search_query: String,
    pending_search: bool,
    pending_play: bool,
    pending_download: bool,
    pending_delete: bool,
    anime_selection_changed: bool,
    bookmarks_refresh_pending: bool,
    filter_changed: bool,
    left_pane_mode: LeftPaneMode,
    filter_mode: FilterMode,
    details_text: String,
    should_quit: bool,
    matcher: Matcher,
    episode_indicators: HashMap<String, EpisodeIndicators>,
}

impl App {
    pub fn new() -> Self {
        Self {
            focus: Focus::Left,
            input_mode: InputMode::Normal,
            left_index: 0,
            right_index: 0,
            selected_anime: None,
            selected_episode: None,
            anime_entries: Vec::new(),
            bookmark_entries: Vec::new(),
            episodes: Vec::new(),
            filtered_episodes: Vec::new(),
            filtered_anime_entries: Vec::new(),
            filtered_bookmark_entries: Vec::new(),
            filtered_episode_entries: Vec::new(),
            panel_filter_mode: false,
            panel_filter_target: None,
            panel_filter_query: String::new(),
            anime_filter_active: false,
            bookmark_filter_active: false,
            episode_filter_active: false,
            anime_filter_query: String::new(),
            bookmark_filter_query: String::new(),
            episode_filter_query: String::new(),
            search_query: DEFAULT_SEARCH_QUERY.to_string(),
            pending_search: false,
            pending_play: false,
            pending_download: false,
            pending_delete: false,
            anime_selection_changed: false,
            bookmarks_refresh_pending: false,
            filter_changed: false,
            left_pane_mode: LeftPaneMode::Search,
            filter_mode: FilterMode::None,
            details_text: concat!(
                "Press s to search, / to filter panels, b for bookmarks, f for filters, ",
                "Space to select, ",
                "d to download, D to delete, q to quit."
            )
            .to_string(),
            should_quit: false,
            matcher: Matcher::new(Config::DEFAULT),
            episode_indicators: HashMap::new(),
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

    pub fn panel_filter_mode(&self) -> bool {
        self.panel_filter_mode
    }

    pub fn panel_filter_target(&self) -> Option<FilterTarget> {
        self.panel_filter_target
    }

    pub fn panel_filter_query(&self) -> &str {
        &self.panel_filter_query
    }

    pub fn panel_filter_active_for(&self, target: FilterTarget) -> bool {
        match target {
            FilterTarget::Anime => self.anime_filter_active,
            FilterTarget::Bookmarks => self.bookmark_filter_active,
            FilterTarget::Episodes => self.episode_filter_active,
        }
    }

    pub fn filter_target_for_focus(&self) -> FilterTarget {
        match self.focus {
            Focus::Left => match self.left_pane_mode {
                LeftPaneMode::Search => FilterTarget::Anime,
                LeftPaneMode::Bookmarks => FilterTarget::Bookmarks,
            },
            Focus::Right => FilterTarget::Episodes,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn enter_panel_filter(&mut self, target: FilterTarget) {
        self.panel_filter_mode = true;
        self.panel_filter_target = Some(target);
        self.panel_filter_query.clear();
        self.set_details("Panel filter: type to narrow results, Enter to apply, Esc to close.");
    }

    pub fn exit_panel_filter(&mut self) {
        self.panel_filter_mode = false;
        self.panel_filter_target = None;
    }

    pub fn update_panel_filter_query(&mut self, query: String) {
        self.panel_filter_query = query;
        if let Some(target) = self.panel_filter_target {
            self.apply_panel_filter_for_target(target);
        }
    }

    pub fn anime_entries(&self) -> &[AnimeEntry] {
        self.visible_anime_entries()
    }

    pub fn bookmark_entries(&self) -> &[FavoriteEntry] {
        self.visible_bookmark_entries()
    }

    pub fn left_pane_mode(&self) -> LeftPaneMode {
        self.left_pane_mode
    }

    pub fn episodes(&self) -> &[Episode] {
        self.visible_episodes()
    }

    pub fn unfiltered_episodes(&self) -> &[Episode] {
        &self.episodes
    }

    pub fn filter_label(&self) -> Option<&'static str> {
        self.filter_mode.label()
    }

    pub fn current_filter(&self) -> Option<PlaybackFilter> {
        self.filter_mode.as_filter()
    }

    pub fn take_filter_changed(&mut self) -> bool {
        if self.filter_changed {
            self.filter_changed = false;
            true
        } else {
            false
        }
    }

    pub fn take_bookmark_refresh(&mut self) -> bool {
        if self.bookmarks_refresh_pending {
            self.bookmarks_refresh_pending = false;
            true
        } else {
            false
        }
    }

    pub fn left_index(&self) -> usize {
        self.left_index
    }

    pub fn right_index(&self) -> usize {
        self.right_index
    }

    pub fn selected_anime(&self) -> Option<usize> {
        self.selected_anime
    }

    pub fn selected_episode(&self) -> Option<usize> {
        self.selected_episode
    }

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn details(&self) -> &str {
        &self.details_text
    }

    pub fn set_episode_indicators(&mut self, indicators: HashMap<String, EpisodeIndicators>) {
        self.episode_indicators = indicators;
    }

    pub fn episode_indicators(&self, episode_id: &str) -> EpisodeIndicators {
        self.episode_indicators
            .get(episode_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn toggle_bookmarks_mode(&mut self) {
        match self.left_pane_mode {
            LeftPaneMode::Search => {
                self.left_pane_mode = LeftPaneMode::Bookmarks;
                self.bookmarks_refresh_pending = true;
                self.reset_navigation_state();
                self.anime_selection_changed = false;
                self.set_details("Bookmarks mode: loading favorites...");
            }
            LeftPaneMode::Bookmarks => {
                self.left_pane_mode = LeftPaneMode::Search;
                self.bookmarks_refresh_pending = false;
                self.reset_navigation_state();
                self.anime_selection_changed = !self.visible_anime_entries().is_empty();
                self.set_details("Search mode: showing results.");
            }
        }
    }

    pub fn load_bookmarks(&mut self, store: &FavoriteStore) {
        self.left_pane_mode = LeftPaneMode::Bookmarks;
        self.bookmark_entries = store.list();
        self.apply_saved_panel_filter(FilterTarget::Bookmarks);
        self.reset_navigation_state();
        if self.bookmark_entries.is_empty() {
            self.set_details("No bookmarks saved yet. Use the CLI to add some.");
            self.anime_selection_changed = false;
        } else {
            self.set_details(format!("Loaded {} bookmarks", self.bookmark_entries.len()));
            self.anime_selection_changed = true;
        }
    }

    pub fn cycle_filter(&mut self) {
        self.filter_mode = self.filter_mode.next();
        self.filter_changed = true;
        self.right_index = 0;
        self.selected_episode = None;
        if let Some(label) = self.filter_mode.label() {
            self.set_details(format!("Filter set to {label}"));
            self.filtered_episodes.clear();
        } else {
            self.set_details("Filters cleared");
            self.clear_filtered_episodes();
        }
    }

    pub fn set_filtered_episodes(&mut self, episodes: Vec<Episode>) {
        self.filtered_episodes = episodes;
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.right_index = 0;
        self.selected_episode = None;
    }

    pub fn clear_filtered_episodes(&mut self) {
        self.filtered_episodes.clear();
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.right_index = 0;
        self.selected_episode = None;
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

    pub fn toggle_focus(&mut self) {
        self.focus = self.focus.toggle();
        self.set_details(match self.focus {
            Focus::Left => "Focus: Anime list",
            Focus::Right => "Focus: Episode list",
        });
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

    pub fn show_help(&mut self) {
        self.set_details(concat!(
            "Controls: s search, / filter panel, b bookmarks, f episode filter, j/k move, h/l focus, ",
            "Space select, d download, D delete, q quit."
        ));
    }

    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    pub fn set_details<S: Into<String>>(&mut self, details: S) {
        self.details_text = details.into();
    }

    pub fn set_search_query<S: Into<String>>(&mut self, query: S) {
        self.search_query = query.into();
    }

    pub fn enter_search_mode(&mut self) {
        let was_bookmarks = matches!(self.left_pane_mode, LeftPaneMode::Bookmarks);
        self.input_mode = InputMode::Search;
        self.left_pane_mode = LeftPaneMode::Search;
        self.bookmarks_refresh_pending = false;
        if self.visible_anime_entries().is_empty() {
            self.left_index = 0;
        } else {
            self.left_index = self
                .left_index
                .min(self.visible_anime_entries().len().saturating_sub(1));
        }
        self.selected_anime = None;
        if was_bookmarks && !self.visible_anime_entries().is_empty() {
            self.anime_selection_changed = true;
        }
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

    pub fn request_play(&mut self) {
        self.pending_play = true;
    }

    pub fn take_pending_play(&mut self) -> bool {
        if self.pending_play {
            self.pending_play = false;
            true
        } else {
            false
        }
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

    pub fn take_anime_selection_changed(&mut self) -> bool {
        if self.anime_selection_changed {
            self.anime_selection_changed = false;
            true
        } else {
            false
        }
    }

    pub fn search(&mut self, client: &AnimeClient<FetchBackend>) -> CoreResult<()> {
        let query = self.search_query.trim().to_owned();
        if query.is_empty() {
            self.left_pane_mode = LeftPaneMode::Search;
            self.bookmarks_refresh_pending = false;
            self.anime_entries.clear();
            self.episodes.clear();
            self.filtered_episodes.clear();
            self.filtered_anime_entries.clear();
            self.filtered_episode_entries.clear();
            self.episode_indicators.clear();
            self.left_index = 0;
            self.right_index = 0;
            self.selected_anime = None;
            self.selected_episode = None;
            self.set_details("Enter a search term with 's'.");
            return Ok(());
        }

        let entries = client.search(&query)?;
        self.anime_entries = entries;
        self.left_pane_mode = LeftPaneMode::Search;
        self.left_index = 0;
        self.selected_anime = None;
        self.anime_selection_changed = false;
        self.right_index = 0;
        self.selected_episode = None;
        self.apply_saved_panel_filter(FilterTarget::Anime);

        if self.anime_entries.is_empty() {
            self.episodes.clear();
            self.filtered_episode_entries.clear();
            self.episode_indicators.clear();
            self.set_details(format!("No results for '{query}'"));
            return Ok(());
        }

        self.set_details(format!("Loaded {} results", self.anime_entries.len()));
        self.load_episodes(client)
    }

    pub fn load_episodes(&mut self, client: &AnimeClient<FetchBackend>) -> CoreResult<()> {
        let Some(anime) = self.current_anime() else {
            self.episodes.clear();
            self.filtered_episodes.clear();
            self.episode_indicators.clear();
            self.right_index = 0;
            self.selected_episode = None;
            self.set_details("Select an anime to load episodes.");
            return Ok(());
        };

        let episodes = client.list_episodes(&anime.id)?;
        self.episodes = episodes;
        self.filtered_episodes.clear();
        self.episode_indicators.clear();
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.right_index = 0;
        self.selected_episode = None;
        self.set_details(format!("Loaded {} episodes", self.episodes.len()));
        self.anime_selection_changed = false;
        Ok(())
    }

    fn reset_navigation_state(&mut self) {
        self.left_index = 0;
        self.right_index = 0;
        self.selected_anime = None;
        self.selected_episode = None;
        self.episodes.clear();
        self.filtered_episodes.clear();
        self.filtered_episode_entries.clear();
        self.episode_indicators.clear();
    }

    fn active_list_len(&self) -> usize {
        match self.focus {
            Focus::Left => self.left_items_len(),
            Focus::Right => self.visible_episodes().len(),
        }
    }

    fn current_anime(&self) -> Option<&AnimeEntry> {
        match self.left_pane_mode {
            LeftPaneMode::Search => self.visible_anime_entries().get(self.left_index),
            LeftPaneMode::Bookmarks => self
                .visible_bookmark_entries()
                .get(self.left_index)
                .map(|entry| &entry.anime),
        }
    }

    pub fn current_episode_id(&self) -> Option<String> {
        self.visible_episodes()
            .get(self.selected_episode.unwrap_or(self.right_index))
            .map(|episode| episode.id.clone())
    }

    pub fn current_episode_title(&self) -> Option<String> {
        self.visible_episodes()
            .get(self.selected_episode.unwrap_or(self.right_index))
            .map(|episode| episode.title.clone())
    }

    fn left_items_len(&self) -> usize {
        match self.left_pane_mode {
            LeftPaneMode::Search => self.visible_anime_entries().len(),
            LeftPaneMode::Bookmarks => self.visible_bookmark_entries().len(),
        }
    }

    fn visible_episodes(&self) -> &[Episode] {
        if self.episode_filter_active {
            &self.filtered_episode_entries
        } else {
            self.base_episode_entries()
        }
    }

    fn base_episode_entries(&self) -> &[Episode] {
        if self.current_filter().is_some() {
            &self.filtered_episodes
        } else {
            &self.episodes
        }
    }

    fn visible_anime_entries(&self) -> &[AnimeEntry] {
        if self.anime_filter_active {
            &self.filtered_anime_entries
        } else {
            &self.anime_entries
        }
    }

    fn visible_bookmark_entries(&self) -> &[FavoriteEntry] {
        if self.bookmark_filter_active {
            &self.filtered_bookmark_entries
        } else {
            &self.bookmark_entries
        }
    }

    fn apply_panel_filter_for_target(&mut self, target: FilterTarget) {
        let query = self.panel_filter_query.clone();
        self.apply_panel_filter_with_query(target, &query);
        *self.saved_query_mut(target) = query;
    }

    fn apply_panel_filter_with_query(&mut self, target: FilterTarget, query: &str) {
        let trimmed = query.trim();
        match target {
            FilterTarget::Anime => {
                self.apply_anime_filter(trimmed);
            }
            FilterTarget::Bookmarks => {
                self.apply_bookmark_filter(trimmed);
            }
            FilterTarget::Episodes => {
                self.apply_episode_filter(trimmed);
            }
        }
    }

    fn apply_anime_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.anime_filter_active = false;
            self.filtered_anime_entries.clear();
            if matches!(self.left_pane_mode, LeftPaneMode::Search) {
                self.left_index = self
                    .left_index
                    .min(self.visible_anime_entries().len().saturating_sub(1));
                self.selected_anime = None;
                self.anime_selection_changed = true;
            }
            return;
        }

        let filtered = fuzzy_filter(&mut self.matcher, &self.anime_entries, query, |entry| {
            entry.title.as_str()
        });
        self.filtered_anime_entries = filtered;
        self.anime_filter_active = true;
        if matches!(self.left_pane_mode, LeftPaneMode::Search) {
            if self.left_index >= self.visible_anime_entries().len() {
                self.left_index = 0;
                self.selected_anime = None;
            }
            self.anime_selection_changed = true;
        }
    }

    fn apply_bookmark_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.bookmark_filter_active = false;
            self.filtered_bookmark_entries.clear();
            if matches!(self.left_pane_mode, LeftPaneMode::Bookmarks) {
                self.left_index = self
                    .left_index
                    .min(self.visible_bookmark_entries().len().saturating_sub(1));
                self.selected_anime = None;
                self.anime_selection_changed = true;
            }
            return;
        }

        let filtered = fuzzy_filter(&mut self.matcher, &self.bookmark_entries, query, |entry| {
            entry.anime.title.as_str()
        });
        self.filtered_bookmark_entries = filtered;
        self.bookmark_filter_active = true;
        if matches!(self.left_pane_mode, LeftPaneMode::Bookmarks) {
            if self.left_index >= self.visible_bookmark_entries().len() {
                self.left_index = 0;
                self.selected_anime = None;
            }
            self.anime_selection_changed = true;
        }
    }

    fn apply_episode_filter(&mut self, query: &str) {
        if query.is_empty() {
            self.episode_filter_active = false;
            self.filtered_episode_entries.clear();
            self.right_index = self
                .right_index
                .min(self.visible_episodes().len().saturating_sub(1));
            self.selected_episode = None;
            return;
        }

        let base = self.base_episode_entries().to_vec();
        let filtered = fuzzy_filter(&mut self.matcher, &base, query, |episode| {
            episode.title.as_str()
        });
        self.filtered_episode_entries = filtered;
        self.episode_filter_active = true;
        if self.right_index >= self.visible_episodes().len() {
            self.right_index = 0;
            self.selected_episode = None;
        }
    }

    fn saved_query_mut(&mut self, target: FilterTarget) -> &mut String {
        match target {
            FilterTarget::Anime => &mut self.anime_filter_query,
            FilterTarget::Bookmarks => &mut self.bookmark_filter_query,
            FilterTarget::Episodes => &mut self.episode_filter_query,
        }
    }

    fn saved_query(&self, target: FilterTarget) -> &str {
        match target {
            FilterTarget::Anime => &self.anime_filter_query,
            FilterTarget::Bookmarks => &self.bookmark_filter_query,
            FilterTarget::Episodes => &self.episode_filter_query,
        }
    }

    fn apply_saved_panel_filter(&mut self, target: FilterTarget) {
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

#[derive(Clone, Copy)]
struct FilterCandidate<'a> {
    index: usize,
    title: &'a str,
}

impl AsRef<str> for FilterCandidate<'_> {
    fn as_ref(&self) -> &str {
        self.title
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
