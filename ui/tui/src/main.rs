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

mod app;
mod events;
mod playback;
mod ui;

use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use animestan_core::{AnimeClient, AppConfig, EpisodeTracker, FavoriteStore};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;
use crate::events::{Event, EventHandler};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    run_app(&mut terminal)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let config = AppConfig::load_default().map_err(to_io_error)?;
    let tracker = Arc::new(Mutex::new(
        EpisodeTracker::load_default(&config).map_err(to_io_error)?,
    ));
    let mut favorites = FavoriteStore::load_default(&config).map_err(to_io_error)?;
    let mut refresh_favorites = false;
    let client = AnimeClient::from_config(&config).map_err(to_io_error)?;

    let mut app = App::new();
    if app.search_query().trim().is_empty() {
        app.set_details("Press / to search for an anime.");
    } else if let Err(err) = app.search(&client) {
        app.set_details(format!("Search failed: {err}"));
    }

    let mut events = EventHandler::new(Duration::from_millis(250));

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        match events.next()? {
            Event::Input(key_event) => app.on_key(key_event),
            Event::Tick => {}
        }

        if app.take_bookmark_refresh() {
            if refresh_favorites {
                match FavoriteStore::load_default(&config) {
                    Ok(store) => {
                        favorites = store;
                    }
                    Err(err) => {
                        app.set_details(format!("Failed to load bookmarks: {err}"));
                    }
                }
            } else {
                refresh_favorites = true;
            }
            app.load_bookmarks(&favorites);
        }

        if app.take_pending_search() {
            match app.search(&client) {
                Ok(()) => {
                    if app.current_filter().is_some() {
                        if let Err(err) = apply_episode_filter(&mut app, &tracker) {
                            app.set_details(format!("Filter failed: {err}"));
                        }
                    }
                }
                Err(err) => {
                    app.set_details(format!("Search failed: {err}"));
                }
            }
        }

        if app.take_anime_selection_changed() {
            if let Err(err) = app.load_episodes(&client) {
                app.set_details(format!("Episode load failed: {err}"));
            } else if app.current_filter().is_some() {
                if let Err(err) = apply_episode_filter(&mut app, &tracker) {
                    app.set_details(format!("Filter failed: {err}"));
                }
            }
        }

        if app.take_filter_changed() {
            if let Err(err) = apply_episode_filter(&mut app, &tracker) {
                app.set_details(format!("Filter failed: {err}"));
            }
        }

        if app.take_pending_play() {
            let Some(episode_id) = app.current_episode_id() else {
                app.set_details("Highlight an episode to play");
                continue;
            };

            let episode_title = app.current_episode_title();
            let stream = match client.resolve_stream_url(&episode_id) {
                Ok(link) => link,
                Err(err) => {
                    app.set_details(format!("Failed to resolve stream: {err}"));
                    continue;
                }
            };

            if let Err(err) = mark_episode_started(&tracker, &episode_id) {
                app.set_details(format!("Playback failed: {err}"));
                continue;
            }

            if let Some(title) = &episode_title {
                app.set_details(format!("Launching player for {title}"));
            } else {
                app.set_details("Launching player...");
            }

            if let Err(err) =
                playback::play_episode(&config, &tracker, &episode_id, stream.url.as_str())
            {
                app.set_details(format!("Playback failed: {err}"));
            } else if let Some(title) = episode_title {
                app.set_details(format!("Finished playing {title}"));
            } else {
                app.set_details("Playback finished");
            }
        }

        if app.should_quit() {
            break;
        }
    }

    Ok(())
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
