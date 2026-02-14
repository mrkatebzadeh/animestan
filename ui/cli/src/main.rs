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

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use spdlog::prelude::*;

use animestan_core::{
    AnimeClient, AnimeEntry, AppConfig, EpisodeTracker, FavoriteEntry, FavoriteStore, FetchBackend,
    PlaybackFilter, delete_episode, download_episode, episode_file_path, init_logging,
    local_playback_url,
};

mod playback;

fn main() -> Result<()> {
    run()
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut config = AppConfig::load_default().context("failed to load configuration")?;
    if cli.vlc {
        config.player = Some(default_vlc_command().to_string());
    }
    init_logging("animestan-cli", cli.verbosity, &config, true)
        .context("failed to initialize logging")?;
    info!("starting CLI command: {}", describe_command(&cli.command));
    let client = AnimeClient::from_config(&config)?;

    match cli.command {
        Commands::Search { query } => handle_search(&client, &query),
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

fn handle_search(client: &AnimeClient<FetchBackend>, query: &str) -> Result<()> {
    let results = client.search(query)?;
    for entry in results {
        println!("{}\t{}", entry.id, entry.title);
    }

    Ok(())
}

fn handle_episodes(
    client: &AnimeClient<FetchBackend>,
    config: &AppConfig,
    anime_id: &str,
    flags: FilterArgs,
) -> Result<()> {
    let episodes = client.list_episodes(anime_id)?;
    if let Some(filter) = flags.selected() {
        let tracker = EpisodeTracker::load_default(config)?;
        let filtered = tracker.filter_episodes(&episodes, filter);
        for episode in filtered {
            println!("{}\t{}\t{}", episode.id, episode.number, episode.title);
        }
    } else {
        for episode in episodes {
            println!("{}\t{}\t{}", episode.id, episode.number, episode.title);
        }
    }

    Ok(())
}

fn handle_url(
    client: &AnimeClient<FetchBackend>,
    config: &AppConfig,
    episode_id: &str,
) -> Result<()> {
    if let Some(local_url) = local_playback_url(config, episode_id) {
        println!("{local_url}");
        return Ok(());
    }

    let link = client
        .resolve_stream_url(episode_id)
        .with_context(|| format!("failed to resolve stream url for '{episode_id}'"))?;
    println!("{}", link.url);
    Ok(())
}

fn handle_play(
    client: &AnimeClient<FetchBackend>,
    config: &AppConfig,
    episode_id: &str,
) -> Result<()> {
    let tracker = Arc::new(Mutex::new(EpisodeTracker::load_default(config)?));
    {
        let mut guard = tracker
            .lock()
            .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
        guard.mark_started(episode_id)?;
    }

    if local_playback_url(config, episode_id).is_some() {
        let local_path = episode_file_path(config, episode_id);
        let local_path_string = local_path.to_string_lossy().into_owned();
        playback::play_episode(config, &tracker, episode_id, local_path_string.as_str())?;
        return Ok(());
    }

    let link = client
        .resolve_stream_url(episode_id)
        .with_context(|| format!("failed to resolve stream url for '{episode_id}'"))?;
    playback::play_episode(config, &tracker, episode_id, link.url.as_str())?;
    Ok(())
}

fn handle_download(
    client: &AnimeClient<FetchBackend>,
    config: &AppConfig,
    episode_id: &str,
) -> Result<()> {
    if let Some(local_url) = local_playback_url(config, episode_id) {
        info!("download requested for '{episode_id}' but file already exists");
        let path = episode_file_path(config, episode_id);
        println!(
            "Episode '{episode_id}' already downloaded at {} ({}), skipping",
            path.display(),
            local_url
        );
        return Ok(());
    }

    info!("downloading episode '{episode_id}' via CLI");
    let link = client
        .resolve_stream_url(episode_id)
        .with_context(|| format!("failed to resolve stream url for '{episode_id}'"))?;
    let saved_path = download_episode(config, episode_id, &link.url)
        .with_context(|| format!("failed to download episode '{episode_id}'"))?;
    info!("episode '{episode_id}' saved to {}", saved_path.display());
    println!("{}", saved_path.display());
    Ok(())
}

fn handle_delete(config: &AppConfig, episode_id: &str) -> Result<()> {
    info!("deleting download for '{episode_id}' via CLI");
    if delete_episode(config, episode_id)
        .with_context(|| format!("failed to delete download for '{episode_id}'"))?
    {
        info!("deleted download for '{episode_id}'");
        println!("Deleted download for '{episode_id}'");
    } else {
        info!("no download found for '{episode_id}'");
        println!("No download found for '{episode_id}'");
    }
    Ok(())
}

fn handle_bookmarks(
    client: &AnimeClient<FetchBackend>,
    config: &AppConfig,
    command: BookmarksCommand,
) -> Result<()> {
    match command {
        BookmarksCommand::Ls {
            unwatched,
            in_progress,
            next,
            recent,
        } => {
            let store = FavoriteStore::load_default(config)?;
            let entries: Vec<FavoriteEntry> = store.list();
            let flags = FilterArgs {
                unwatched,
                in_progress,
                next,
                recent,
            };
            if let Some(filter) = flags.selected() {
                let tracker = EpisodeTracker::load_default(config)?;
                for favorite in entries {
                    let episodes = client.list_episodes(&favorite.anime.id)?;
                    let filtered = tracker.filter_episodes(&episodes, filter);
                    if filtered.is_empty() {
                        continue;
                    }

                    match filter {
                        PlaybackFilter::Unwatched | PlaybackFilter::Next => {
                            if let Some(episode) = filtered.first().cloned() {
                                println!(
                                    "{}\t{}\t{}\t{}",
                                    favorite.anime.id,
                                    favorite.anime.title,
                                    episode.id,
                                    episode.title
                                );
                            }
                        }
                        PlaybackFilter::InProgress => {
                            if let Some(episode) = tracker.most_recent_episode(&filtered) {
                                println!(
                                    "{}\t{}\t{}\t{}",
                                    favorite.anime.id,
                                    favorite.anime.title,
                                    episode.id,
                                    episode.title
                                );
                            }
                        }
                        PlaybackFilter::Recent => {
                            if let Some(episode) = tracker.most_recent_episode(&episodes) {
                                println!(
                                    "{}\t{}\t{}\t{}",
                                    favorite.anime.id,
                                    favorite.anime.title,
                                    episode.id,
                                    episode.title
                                );
                            }
                        }
                    }
                }
            } else {
                for favorite in entries {
                    println!("{}\t{}", favorite.anime.id, favorite.anime.title);
                }
            }
        }
        BookmarksCommand::Add { anime_id, title } => {
            let mut store = FavoriteStore::load_default(config)?;
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
            let mut store = FavoriteStore::load_default(config)?;
            let removed = store.remove(&anime_id)?;
            if removed {
                println!("Removed bookmark '{anime_id}'");
            } else {
                println!("No bookmark found for '{anime_id}'");
            }
        }
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
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
    #[arg(
        long,
        global = true,
        help = "Use VLC for playback (overrides AppConfig::player)."
    )]
    vlc: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search catalog by query string
    Search { query: String },
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

#[derive(Clone, Copy, Debug)]
enum FilterChoice {
    Unwatched,
    InProgress,
    Next,
    Recent,
}

impl From<FilterChoice> for PlaybackFilter {
    fn from(choice: FilterChoice) -> Self {
        match choice {
            FilterChoice::Unwatched => PlaybackFilter::Unwatched,
            FilterChoice::InProgress => PlaybackFilter::InProgress,
            FilterChoice::Next => PlaybackFilter::Next,
            FilterChoice::Recent => PlaybackFilter::Recent,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default)]
struct FilterArgs {
    unwatched: bool,
    in_progress: bool,
    next: bool,
    recent: bool,
}

impl FilterArgs {
    fn selected(self) -> Option<PlaybackFilter> {
        let choice = if self.unwatched {
            Some(FilterChoice::Unwatched)
        } else if self.in_progress {
            Some(FilterChoice::InProgress)
        } else if self.next {
            Some(FilterChoice::Next)
        } else if self.recent {
            Some(FilterChoice::Recent)
        } else {
            None
        };

        choice.map(PlaybackFilter::from)
    }
}

fn describe_command(command: &Commands) -> &'static str {
    match command {
        Commands::Search { .. } => "search",
        Commands::Episodes { .. } => "episodes",
        Commands::Url { .. } => "url",
        Commands::Play { .. } => "play",
        Commands::Download { .. } => "download",
        Commands::Delete { .. } => "delete",
        Commands::Bookmarks { command } => match command {
            BookmarksCommand::Ls { .. } => "bookmarks::ls",
            BookmarksCommand::Add { .. } => "bookmarks::add",
            BookmarksCommand::Rm { .. } => "bookmarks::rm",
        },
    }
}

fn default_vlc_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "vlc.exe"
    } else {
        "vlc"
    }
}

#[cfg(test)]
mod tests {
    use super::default_vlc_command;

    #[test]
    fn default_vlc_command_matches_platform() {
        if cfg!(target_os = "windows") {
            assert_eq!(default_vlc_command(), "vlc.exe");
        } else {
            assert_eq!(default_vlc_command(), "vlc");
        }
    }
}
