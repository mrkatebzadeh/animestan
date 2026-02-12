// Copyright (C) 2026 M.R. Siavash Katebzadeg <mr@katebzadeh.xyz>
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

use animestan_core::{
    AnimeClient, AnimeEntry, AppConfig, EpisodeTracker, FavoriteEntry, FavoriteStore,
};
use clap::{Parser, Subcommand};
use std::sync::{Arc, Mutex};

mod playback;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = AppConfig::load_default()?;
    let client = AnimeClient::from_config(&config)?;

    match cli.command {
        Commands::Search { query } => {
            let results = client.search(&query)?;
            for entry in results {
                println!("{}\t{}", entry.id, entry.title);
            }
        }
        Commands::Episodes { anime_id } => {
            let episodes = client.list_episodes(&anime_id)?;
            for episode in episodes {
                println!("{}\t{}\t{}", episode.id, episode.number, episode.title);
            }
        }
        Commands::Url { episode_id } => {
            let link = client.resolve_stream_url(&episode_id)?;
            println!("{}", link.url);
        }
        Commands::Play { episode_id } => {
            let link = client.resolve_stream_url(&episode_id)?;
            let tracker = Arc::new(Mutex::new(EpisodeTracker::load_default(&config)?));
            {
                let mut guard = tracker
                    .lock()
                    .map_err(|_| std::io::Error::other("episode tracker lock poisoned"))?;
                guard.mark_started(&episode_id)?;
            }

            playback::play_episode(&config, &tracker, &episode_id, link.url.as_str())?;
        }
        Commands::Bookmarks { command } => match command {
            BookmarksCommand::Ls => {
                let store = FavoriteStore::load_default(&config)?;
                let entries: Vec<FavoriteEntry> = store.list();
                for favorite in entries {
                    println!("{}\t{}", favorite.anime.id, favorite.anime.title);
                }
            }
            BookmarksCommand::Add { anime_id, title } => {
                let mut store = FavoriteStore::load_default(&config)?;
                let source_id = config
                    .source_id
                    .clone()
                    .unwrap_or_else(|| "allanime".to_string());
                let anime_entry = AnimeEntry {
                    id: anime_id.clone(),
                    title: title.unwrap_or_else(|| anime_id.clone()),
                    source_id,
                };
                store.add(anime_entry)?;
                println!("Added bookmark '{anime_id}'");
            }
            BookmarksCommand::Rm { anime_id } => {
                let mut store = FavoriteStore::load_default(&config)?;
                let removed = store.remove(&anime_id)?;
                if removed {
                    println!("Removed bookmark '{anime_id}'");
                } else {
                    println!("No bookmark found for '{anime_id}'");
                }
            }
        },
    }

    Ok(())
}

const ABOUT: &str = concat!(
    "Search live AllAnime by default. ",
    "Edit ~/.config/animestan/config.toml or set ANIMESTAN_USE_FIXTURES=1 to use fixtures."
);

#[derive(Parser)]
#[command(name = "animestan-cli", version, about = ABOUT, long_about = ABOUT)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search catalog by query string
    Search { query: String },
    /// List episodes for a given anime id
    Episodes { anime_id: String },
    /// Resolve a stream URL for an episode
    Url { episode_id: String },
    /// Resolve and play an episode via the configured player
    Play { episode_id: String },
    /// Manage bookmarks
    Bookmarks {
        #[command(subcommand)]
        command: BookmarksCommand,
    },
}

#[derive(Subcommand)]
enum BookmarksCommand {
    /// List saved bookmarks
    Ls,
    /// Add a bookmark by anime id
    Add {
        anime_id: String,
        #[arg(long)]
        title: Option<String>,
    },
    /// Remove a bookmark by anime id
    Rm { anime_id: String },
}
