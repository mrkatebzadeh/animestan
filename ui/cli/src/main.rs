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

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use spdlog::prelude::*;

use animestan_core::{AnimeClient, AppConfig, init_logging};

mod commands;
mod playback;

use crate::commands::{
    FilterArgs, describe_command, handle_bookmarks, handle_delete, handle_download,
    handle_episodes, handle_info, handle_play, handle_search, handle_url,
};

fn main() -> Result<()> {
    run()
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load_default().context("failed to load configuration")?;
    init_logging("animestan-cli", cli.verbosity, &config, true)
        .context("failed to initialize logging")?;
    info!("starting CLI command: {}", describe_command(&cli.command));
    let client = AnimeClient::from_config(&config)?;

    match cli.command {
        Commands::Search { query } => handle_search(&client, &query),
        Commands::Info { title } => handle_info(&title),
        Commands::Episodes {
            anime_id,
            unwatched,
            in_progress,
            next,
            recent,
        } => handle_episodes(
            &client,
            &config,
            &anime_id,
            FilterArgs {
                unwatched,
                in_progress,
                next,
                recent,
            },
        ),
        Commands::Url { episode_id } => handle_url(&client, &config, &episode_id),
        Commands::Play { episode_id } => handle_play(&client, &config, &episode_id),
        Commands::Download { episode_id } => handle_download(&client, &config, &episode_id),
        Commands::Delete { episode_id } => handle_delete(&config, &episode_id),
        Commands::Bookmarks { command } => handle_bookmarks(&client, &config, command),
    }?;

    Ok(())
}

const ABOUT: &str = concat!(
    "Search live AllAnime by default. ",
    "Edit ~/.config/animestan/config.toml or set ANIMESTAN_USE_FIXTURES=1 to use fixtures."
);

#[derive(Parser)]
#[command(name = "animestan-cli", version, about = ABOUT, long_about = ABOUT)]
struct Cli {
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search catalog by query string
    Search { query: String },
    /// Show metadata for a given anime title
    Info { title: String },
    /// List episodes for a given anime id
    Episodes {
        anime_id: String,
        #[arg(long, conflicts_with_all = ["in_progress", "next", "recent"])]
        unwatched: bool,
        #[arg(long, conflicts_with_all = ["unwatched", "next", "recent"])]
        in_progress: bool,
        #[arg(long, conflicts_with_all = ["unwatched", "in_progress", "recent"])]
        next: bool,
        #[arg(long, conflicts_with_all = ["unwatched", "in_progress", "next"])]
        recent: bool,
    },
    /// Resolve a stream URL for an episode, preferring local downloads when available
    Url { episode_id: String },
    /// Resolve (or reuse downloads) and play an episode via the configured player
    Play { episode_id: String },
    /// Download an episode for offline playback
    Download { episode_id: String },
    /// Delete a previously downloaded episode
    Delete { episode_id: String },
    /// Manage bookmarks
    Bookmarks {
        #[command(subcommand)]
        command: BookmarksCommand,
    },
}

#[derive(Subcommand)]
enum BookmarksCommand {
    /// List saved bookmarks
    Ls {
        #[arg(long, conflicts_with_all = ["in_progress", "next", "recent"])]
        unwatched: bool,
        #[arg(long, conflicts_with_all = ["unwatched", "next", "recent"])]
        in_progress: bool,
        #[arg(long, conflicts_with_all = ["unwatched", "in_progress", "recent"])]
        next: bool,
        #[arg(long, conflicts_with_all = ["unwatched", "in_progress", "next"])]
        recent: bool,
    },
    /// Add a bookmark by anime id
    Add {
        anime_id: String,
        #[arg(long)]
        title: Option<String>,
    },
    /// Remove a bookmark by anime id
    Rm { anime_id: String },
}
