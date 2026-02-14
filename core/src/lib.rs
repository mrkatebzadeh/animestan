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

mod client;
mod config;
mod downloads;
mod error;
mod favorites;
mod fixtures;
mod logging;
mod models;
mod source;
mod tracking;

pub use crate::client::{AnimeClient, FetchBackend, Fetcher};
pub use crate::config::AppConfig;
pub use crate::downloads::{
    delete_episode, download_episode, episode_file_path, local_playback_url,
};
pub use crate::error::Error;
pub type CoreResult<T> = anyhow::Result<T>;
pub use crate::favorites::{FavoriteEntry, FavoriteStore};
pub use crate::logging::app_log_path;
pub use crate::logging::init_logging;
pub use crate::models::{AnimeEntry, Episode, EpisodePlaybackState, PlaybackFilter, StreamLink};
pub use crate::tracking::EpisodeTracker;
pub use anyhow::Result as AnyhowResult;
