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

use std::sync::{Arc, Mutex};

use anyhow::Result;

use animestan_core::{AppConfig, EpisodeTracker, PlayerOutput, play_episode as core_play_episode};

/// Plays an episode using the CLI playback policy.
///
/// # Errors
///
/// Propagates all playback and tracker-update errors from
/// [`animestan_core::play_episode`].
pub fn play_episode(
    config: &AppConfig,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    stream_url: &str,
) -> Result<()> {
    core_play_episode(
        config,
        tracker,
        episode_id,
        stream_url,
        PlayerOutput::Inherit,
    )
}
