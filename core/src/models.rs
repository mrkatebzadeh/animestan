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

use serde::{Deserialize, Serialize};
use url::Url;

pub type SourceId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeEntry {
    pub id: String,
    pub title: String,
    pub source_id: SourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub anime_id: String,
    pub source_id: SourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackFilter {
    Unwatched,
    InProgress,
    Next,
    Recent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpisodePlaybackState {
    pub watched: bool,
    pub in_progress: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamLink {
    pub url: Url,
    pub episode_id: String,
    pub source_id: SourceId,
}
