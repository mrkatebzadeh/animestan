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

mod app;
mod events;
mod playback;
mod ui;

use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use animestan_core::{
    AnimeClient, AppConfig, EpisodeTracker, FavoriteStore, FetchBackend, delete_episode,
    download_episode, episode_file_path, init_logging, local_playback_url,
};
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use spdlog::prelude::*;

use crate::app::App;
use crate::events::{Event, EventHandler};

fn main() -> io::Result<()> {
    let args = Args::parse();
    let config = AppConfig::load_default().map_err(to_io_error)?;
    init_logging("animestan-tui", args.verbosity, &config, false).map_err(to_io_error)?;
    info!("launching animestan-tui");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    run_app(&mut terminal, &config)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: &AppConfig,
) -> io::Result<()> {
    let tracker = Arc::new(Mutex::new(
        EpisodeTracker::load_default(config).map_err(to_io_error)?,
    ));
    let mut favorites = FavoriteStore::load_default(config).map_err(to_io_error)?;
    let mut refresh_favorites = false;
    let client = AnimeClient::from_config(config).map_err(to_io_error)?;

    let mut app = App::new();
    initialize_app(&mut app, &client);

    let mut events = EventHandler::new(Duration::from_millis(250));

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        match events.next()? {
            Event::Input(key_event) => app.on_key(key_event),
            Event::Tick => {}
        }

        handle_bookmarks_refresh(&mut app, config, &mut favorites, &mut refresh_favorites);

        handle_search(&mut app, &client, &tracker);
        handle_filters(&mut app, &client, &tracker);

        if handle_download(&mut app, config, &client) {
            continue;
        }

        if handle_delete(&mut app, config) {
            continue;
        }

        if handle_playback(&mut app, config, &tracker, &client) {
            continue;
        }

        if app.should_quit() {
            break;
        }
    }

    Ok(())
}

fn initialize_app(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if app.search_query().trim().is_empty() {
        app.set_details("Press / to search for an anime.");
    } else if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
}

fn handle_bookmarks_refresh(
    app: &mut App,
    config: &AppConfig,
    favorites: &mut FavoriteStore,
    refresh_favorites: &mut bool,
) {
    if !app.take_bookmark_refresh() {
        return;
    }

    if *refresh_favorites {
        match FavoriteStore::load_default(config) {
            Ok(store) => {
                *favorites = store;
            }
            Err(err) => {
                app.set_details(format!("Failed to load bookmarks: {err}"));
            }
        }
    } else {
        *refresh_favorites = true;
    }

    app.load_bookmarks(favorites);
}

fn handle_search(
    app: &mut App,
    client: &AnimeClient<FetchBackend>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
) {
    if !app.take_pending_search() {
        return;
    }

    match app.search(client) {
        Ok(()) => {
            if app.current_filter().is_some() {
                if let Err(err) = apply_episode_filter(app, tracker) {
                    app.set_details(format!("Filter failed: {err}"));
                }
            }
        }
        Err(err) => {
            app.set_details(format!("Search failed: {err}"));
        }
    }
}

fn handle_filters(
    app: &mut App,
    client: &AnimeClient<FetchBackend>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
) {
    let mut apply_filter = false;

    if app.take_anime_selection_changed() {
        match app.load_episodes(client) {
            Ok(()) => {
                if app.current_filter().is_some() {
                    apply_filter = true;
                }
            }
            Err(err) => {
                app.set_details(format!("Episode load failed: {err}"));
            }
        }
    }

    if app.take_filter_changed() {
        apply_filter = true;
    }

    if apply_filter {
        if let Err(err) = apply_episode_filter(app, tracker) {
            app.set_details(format!("Filter failed: {err}"));
        }
    }
}

fn handle_download(app: &mut App, config: &AppConfig, client: &AnimeClient<FetchBackend>) -> bool {
    if !app.take_pending_download() {
        return false;
    }

    let Some(episode_id) = app.current_episode_id() else {
        app.set_details("Highlight an episode to download.");
        return true;
    };

    info!("download requested from TUI for '{episode_id}'");
    let episode_title = app.current_episode_title();

    if local_playback_url(config, &episode_id).is_some() {
        let path = episode_file_path(config, &episode_id);
        app.set_details(format!("Episode already downloaded at {}", path.display()));
        info!(
            "episode '{episode_id}' already downloaded locally at {}",
            path.display()
        );
        return true;
    }

    let stream = match client.resolve_stream_url(&episode_id) {
        Ok(link) => link,
        Err(err) => {
            app.set_details(format!("Failed to resolve stream: {err}"));
            return true;
        }
    };

    match download_episode(config, &episode_id, &stream.url) {
        Ok(saved_path) => {
            info!(
                "downloaded episode '{episode_id}' to {}",
                saved_path.display()
            );
            if let Some(title) = episode_title {
                app.set_details(format!("Downloaded {title} to {}", saved_path.display()));
            } else {
                app.set_details(format!("Download saved to {}", saved_path.display()));
            }
        }
        Err(err) => {
            app.set_details(format!("Download failed: {err}"));
        }
    }

    false
}

fn handle_delete(app: &mut App, config: &AppConfig) -> bool {
    if !app.take_pending_delete() {
        return false;
    }

    let Some(episode_id) = app.current_episode_id() else {
        app.set_details("Highlight an episode to delete its download.");
        return true;
    };

    info!("delete requested from TUI for '{episode_id}'");
    let episode_title = app.current_episode_title();
    match delete_episode(config, &episode_id) {
        Ok(true) => {
            info!("deleted download for '{episode_id}'");
            if let Some(title) = episode_title {
                app.set_details(format!("Deleted download for {title}"));
            } else {
                app.set_details("Deleted download.");
            }
        }
        Ok(false) => {
            info!("no download found for '{episode_id}'");
            app.set_details("No download found to delete.");
        }
        Err(err) => {
            app.set_details(format!("Delete failed: {err}"));
        }
    }

    false
}

fn handle_playback(
    app: &mut App,
    config: &AppConfig,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    client: &AnimeClient<FetchBackend>,
) -> bool {
    if !app.take_pending_play() {
        return false;
    }

    let Some(episode_id) = app.current_episode_id() else {
        app.set_details("Highlight an episode to play");
        return true;
    };

    let episode_title = app.current_episode_title();
    let (target_url, using_local): (String, bool) =
        if local_playback_url(config, &episode_id).is_some() {
            let path = episode_file_path(config, &episode_id);
            (path.to_string_lossy().into_owned(), true)
        } else {
            let stream = match client.resolve_stream_url(&episode_id) {
                Ok(link) => link,
                Err(err) => {
                    app.set_details(format!("Failed to resolve stream: {err}"));
                    return true;
                }
            };
            (stream.url.to_string(), false)
        };

    info!(
        "playback requested for '{episode_id}' using {} source",
        if using_local { "local" } else { "remote" }
    );

    if let Err(err) = mark_episode_started(tracker, &episode_id) {
        app.set_details(format!("Playback failed: {err}"));
        return true;
    }

    if let Some(title) = &episode_title {
        if using_local {
            app.set_details(format!("Launching local playback for {title}"));
        } else {
            app.set_details(format!("Launching player for {title}"));
        }
    } else if using_local {
        app.set_details("Launching local playback...");
    } else {
        app.set_details("Launching player...");
    }

    if let Err(err) = playback::play_episode(config, tracker, &episode_id, target_url.as_str()) {
        app.set_details(format!("Playback failed: {err}"));
    } else if let Some(title) = episode_title {
        app.set_details(format!("Finished playing {title}"));
    } else {
        app.set_details("Playback finished");
    }

    false
}

#[derive(Parser)]
struct Args {
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
}

fn to_io_error(err: animestan_core::Error) -> io::Error {
    io::Error::other(err)
}

fn apply_episode_filter(app: &mut App, tracker: &Arc<Mutex<EpisodeTracker>>) -> io::Result<()> {
    if let Some(filter) = app.current_filter() {
        let filtered = {
            let guard = tracker
                .lock()
                .map_err(|_| io::Error::other("episode tracker lock poisoned"))?;
            guard.filter_episodes(app.unfiltered_episodes(), filter)
        };
        app.set_filtered_episodes(filtered);
    } else {
        app.clear_filtered_episodes();
    }

    Ok(())
}

fn mark_episode_started(tracker: &Arc<Mutex<EpisodeTracker>>, episode_id: &str) -> io::Result<()> {
    let mut guard = tracker
        .lock()
        .map_err(|_| io::Error::other("episode tracker lock poisoned"))?;
    guard.mark_started(episode_id).map_err(to_io_error)
}
