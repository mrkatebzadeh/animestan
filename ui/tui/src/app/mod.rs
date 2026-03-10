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

use std::collections::{HashMap, HashSet, VecDeque};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Focus {
    #[default]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Search,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EpisodeMarkAction {
    Current { watched: bool },
    All { watched: bool },
    UpToCurrent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PlaybackStatus {
    #[default]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FilterMode {
    #[default]
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

#[derive(Clone, Debug, Default)]
pub struct MetadataSummary {
    pub status: Option<String>,
    pub score: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default)]
enum PanelMode {
    #[default]
    Inactive,
    Active,
}

#[derive(Clone, Copy, Debug, Default)]
enum FilterActive {
    #[default]
    Inactive,
    Active,
}

#[derive(Clone, Copy, Debug, Default)]
enum SearchModal {
    #[default]
    Hidden,
    Visible,
}

#[derive(Clone, Copy, Debug, Default)]
enum MetaFetch {
    #[default]
    Idle,
    Pending,
    Loading,
}

#[derive(Clone, Copy, Debug, Default)]
enum PendingFlag {
    #[default]
    No,
    Yes,
}

#[derive(Clone, Copy, Debug, Default)]
enum Progress {
    #[default]
    Idle,
    Active,
}

#[derive(Debug, Default)]
struct NavState {
    focus: Focus,
    input_mode: InputMode,
    left_index: usize,
    right_index: usize,
    selected_anime: Option<usize>,
    selected_episode: Option<usize>,
    pending_double_g: bool,
    anime_selection_changed: bool,
}

#[derive(Debug, Default)]
struct FilterState {
    panel_mode: PanelMode,
    panel_target: Option<FilterTarget>,
    panel_query: String,
    bookmark_active: FilterActive,
    episode_active: FilterActive,
    bookmark_query: String,
    episode_query: String,
    filter_changed: PendingFlag,
    filter_mode: FilterMode,
}

#[derive(Debug, Default)]
struct DataState {
    search_results: Vec<AnimeEntry>,
    bookmark_entries: Vec<FavoriteEntry>,
    episodes: Vec<Episode>,
    filtered_episodes: Vec<Episode>,
    filtered_bookmark_entries: Vec<FavoriteEntry>,
    filtered_episode_entries: Vec<Episode>,
    episodes_loading: bool,
    fetch_generation: u64,
    episode_indicators: HashMap<String, EpisodeIndicators>,
    anime_progress: HashMap<String, AnimeProgress>,
    metadata_store: HashMap<String, AnimeMetadata>,
    metadata_pending: HashSet<String>,
    metadata_failed: HashSet<String>,
    episode_refresh_pending: HashSet<String>,
}

#[derive(Debug, Default)]
struct SearchState {
    query: String,
    pending_search: PendingFlag,
    results_query: String,
    modal_visible: SearchModal,
    selection: usize,
    metadata: Option<AnimeMetadata>,
    metadata_error: Option<String>,
    metadata_generation: u64,
    meta_state: MetaFetch,
    add_pending: PendingFlag,
}

#[derive(Debug, Default)]
struct PlaybackState {
    status: PlaybackStatus,
    in_progress: Progress,
    current_episode_id: Option<String>,
    current_anime_title: Option<String>,
    current_episode_title: Option<String>,
    elapsed_seconds: Option<f64>,
    pending_playback_request: PendingFlag,
    pending_download: PendingFlag,
    pending_delete: PendingFlag,
    pending_bookmark_toggle: PendingFlag,
    pending_episode_mark_action: Option<EpisodeMarkAction>,
}

#[derive(Debug, Default)]
struct QuickLaunchState {
    active: PendingFlag,
    query: String,
    selection: usize,
    items: Vec<QuickLaunchCandidate>,
    history: VecDeque<String>,
    recently_played: VecDeque<String>,
    last_played_episode: Option<LastPlayedEpisode>,
    pending_playback_override: Option<PendingPlayback>,
}

#[derive(Debug, Default)]
struct ModalState {
    info_visible: bool,
    info_loading: bool,
    info_metadata: Option<AnimeMetadata>,
    info_error: Option<String>,
    pending_info_fetch: bool,
    info_fetch_generation: u64,
}

#[derive(Debug, Default)]
struct KeybindingsState {
    scroll: usize,
    content_lines: usize,
    viewport_lines: usize,
}

#[derive(Debug)]
struct UiState {
    details_text: String,
    should_quit: bool,
    confirm_exit: bool,
    confirm_exit_choice: ConfirmExitChoice,
    show_keybindings: bool,
}

#[derive(Debug, Default)]
struct MetadataBackgroundState {
    pending: usize,
    spinner_index: usize,
}

pub struct App {
    nav: NavState,
    filters: FilterState,
    data: DataState,
    search: SearchState,
    playback: PlaybackState,
    quick: QuickLaunchState,
    modal: ModalState,
    keybindings: KeybindingsState,
    ui: UiState,
    metadata_background: MetadataBackgroundState,
    matcher: Matcher,
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
