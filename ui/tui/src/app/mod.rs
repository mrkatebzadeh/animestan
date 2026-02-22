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

use std::collections::{HashMap, VecDeque};

use animestan_core::{AnimeEntry, AnimeMetadata, Episode, FavoriteEntry, PlaybackFilter};
use nucleo::Matcher;
use quick::{LastPlayedEpisode, PendingPlayback};

mod core;
mod data;
mod filter;
mod modal;
mod nav;
mod playback;
mod quick;
mod search;

#[allow(unused_imports)]
pub use quick::{QuickLaunchAction, QuickLaunchCandidate};

const DEFAULT_SEARCH_QUERY: &str = "";
const QUICK_LAUNCH_HISTORY_SIZE: usize = 12;
const QUICK_LAUNCH_RECENT_PLAY_SIZE: usize = 8;

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
pub enum ConfirmExitChoice {
    Yes,
    No,
}

impl ConfirmExitChoice {
    const fn toggle(self) -> Self {
        match self {
            Self::Yes => Self::No,
            Self::No => Self::Yes,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EpisodeMarkAction {
    Current { watched: bool },
    All { watched: bool },
    UpToCurrent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackStatus {
    None,
    Playing,
    Downloading,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FilterTarget {
    Anime,
    Episodes,
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

#[derive(Clone, Copy, Debug)]
pub struct AnimeProgress {
    pub watched: usize,
    pub total: usize,
}

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    focus: Focus,
    input_mode: InputMode,
    left_index: usize,
    right_index: usize,
    selected_anime: Option<usize>,
    selected_episode: Option<usize>,
    search_results: Vec<AnimeEntry>,
    bookmark_entries: Vec<FavoriteEntry>,
    episodes: Vec<Episode>,
    filtered_episodes: Vec<Episode>,
    filtered_bookmark_entries: Vec<FavoriteEntry>,
    filtered_episode_entries: Vec<Episode>,
    panel_filter_mode: bool,
    panel_filter_target: Option<FilterTarget>,
    panel_filter_query: String,
    bookmark_filter_active: bool,
    episode_filter_active: bool,
    bookmark_filter_query: String,
    episode_filter_query: String,
    episodes_loading: bool,
    fetch_generation: u64,
    search_query: String,
    pending_search: bool,
    pending_playback_request: bool,
    pending_download: bool,
    pending_delete: bool,
    pending_bookmark_toggle: bool,
    pending_episode_mark_action: Option<EpisodeMarkAction>,
    pending_double_g: bool,
    anime_selection_changed: bool,
    filter_changed: bool,
    filter_mode: FilterMode,
    playback_status: PlaybackStatus,
    playback_in_progress: bool,
    current_playing_episode_id: Option<String>,
    current_playing_anime_title: Option<String>,
    current_playing_episode_title: Option<String>,
    playback_elapsed_seconds: Option<f64>,
    details_text: String,
    should_quit: bool,
    confirm_exit: bool,
    confirm_exit_choice: ConfirmExitChoice,
    show_keybindings: bool,
    matcher: Matcher,
    episode_indicators: HashMap<String, EpisodeIndicators>,
    quick_launch_active: bool,
    quick_launch_query: String,
    anime_progress: HashMap<String, AnimeProgress>,
    quick_launch_selection: usize,
    quick_launch_items: Vec<QuickLaunchCandidate>,
    quick_launch_history: VecDeque<String>,
    quick_launch_recently_played: VecDeque<String>,
    last_played_episode: Option<LastPlayedEpisode>,
    pending_playback_override: Option<PendingPlayback>,
    info_modal_visible: bool,
    info_modal_loading: bool,
    info_modal_metadata: Option<AnimeMetadata>,
    info_modal_error: Option<String>,
    pending_info_fetch: bool,
    info_fetch_generation: u64,
    search_results_query: String,
    search_results_modal_visible: bool,
    search_results_selection: usize,
    search_results_metadata: Option<AnimeMetadata>,
    search_results_metadata_error: Option<String>,
    search_results_metadata_loading: bool,
    search_results_metadata_generation: u64,
    search_results_metadata_pending: bool,
    search_results_add_pending: bool,
    keybindings_scroll: usize,
    keybindings_content_lines: usize,
    keybindings_viewport_lines: usize,
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
