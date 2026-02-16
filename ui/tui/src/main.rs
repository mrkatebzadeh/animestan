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

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use animestan_core::{
    AnimeClient, AppConfig, CoreResult, Episode, EpisodeTracker, FavoriteStore, FetchBackend,
    delete_episode, download_episode, episode_file_path, init_logging, local_playback_url,
};
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::future::{AbortHandle, Abortable};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use spdlog::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender, error::TryRecvError, unbounded_channel,
};
use tokio::time::sleep;

use crate::app::{App, EpisodeIndicators, LeftPaneMode, PlaybackStatus};
use crate::events::{Event, EventHandler};

struct EpisodeFetchRequest {
    generation: u64,
    anime_id: String,
}

struct EpisodeFetchResult {
    generation: u64,
    result: CoreResult<Vec<Episode>>,
}

#[derive(Clone)]
struct PlaybackRequest {
    episode_id: String,
    episode_title: Option<String>,
}

struct PlaybackResult {
    episode_title: Option<String>,
    outcome: Result<()>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Arc::new(AppConfig::load_default().context("failed to load configuration")?);
    init_logging("animestan-tui", args.verbosity, &config, false)
        .context("failed to initialize logging")?;
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

#[allow(clippy::too_many_lines)]
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: &Arc<AppConfig>,
) -> Result<()> {
    let tracker = Arc::new(Mutex::new(
        EpisodeTracker::load_default(config).context("failed to load episode tracker")?,
    ));
    let mut favorites = FavoriteStore::load_default(config).context("failed to load favorites")?;
    let mut refresh_favorites = false;
    let client = Arc::new(AnimeClient::from_config(config.as_ref())?);
    let runtime = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    let runtime_handle = runtime.handle().clone();
    let (request_tx, mut request_rx) = unbounded_channel::<EpisodeFetchRequest>();
    let (result_tx, mut result_rx) = unbounded_channel::<EpisodeFetchResult>();
    let mut active_fetch: Option<AbortHandle> = None;
    let (playback_request_tx, mut playback_request_rx) = unbounded_channel::<PlaybackRequest>();
    let (playback_result_tx, mut playback_result_rx) = unbounded_channel::<PlaybackResult>();
    let mut active_playback: Option<AbortHandle> = None;

    let mut app = App::new();
    app.sync_bookmark_cache(&favorites);
    initialize_app(&mut app, client.as_ref());
    if let Err(err) = refresh_episode_indicators(&mut app, &tracker, config) {
        app.set_details(format!("Failed to refresh indicators: {err}"));
    }

    let mut events = EventHandler::new(Duration::from_millis(250));

    loop {
        update_playback_elapsed(&mut app, &tracker);
        terminal.draw(|frame| ui::render(frame, &app))?;

        match events.next()? {
            Event::Input(key_event) => app.on_key(key_event),
            Event::Tick => {}
        }

        if app.take_pending_bookmark_toggle() {
            match app.toggle_bookmark(&mut favorites) {
                Ok(()) => {
                    if matches!(app.left_pane_mode(), LeftPaneMode::Bookmarks) {
                        let details = app.details().to_string();
                        app.load_bookmarks(&favorites);
                        app.set_details(details);
                    }
                }
                Err(err) => {
                    app.set_details(format!("Bookmark toggle failed: {err}"));
                }
            }
        }

        handle_bookmarks_refresh(&mut app, config, &mut favorites, &mut refresh_favorites);

        handle_search(&mut app, client.as_ref());
        handle_filters(&mut app, &tracker, &request_tx);

        loop {
            match request_rx.try_recv() {
                Ok(request) => {
                    if let Some(handle) = active_fetch.take() {
                        handle.abort();
                    }
                    let abort_handle = spawn_episode_fetch_task(
                        &runtime_handle,
                        Arc::clone(&client),
                        request,
                        result_tx.clone(),
                    );
                    active_fetch = Some(abort_handle);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    app.set_details("Episode fetch queue disconnected.");
                    app.set_episodes_loading(false);
                    break;
                }
            }
        }

        loop {
            match result_rx.try_recv() {
                Ok(fetch_result) => {
                    if fetch_result.generation != app.current_fetch_generation() {
                        continue;
                    }

                    match fetch_result.result {
                        Ok(episodes) => {
                            let count = episodes.len();
                            app.set_episodes(episodes);
                            app.set_details(format!("Loaded {count} episodes"));
                            if app.current_filter().is_some() {
                                if let Err(err) = apply_episode_filter(&mut app, &tracker) {
                                    app.set_details(format!("Filter failed: {err}"));
                                }
                            }
                            if let Err(err) = refresh_episode_indicators(&mut app, &tracker, config)
                            {
                                app.set_details(format!("Failed to refresh indicators: {err}"));
                            }
                        }
                        Err(err) => {
                            app.set_episodes_loading(false);
                            app.set_details(format!("Episode load failed: {err}"));
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    app.set_details("Episode fetch worker disconnected.");
                    app.set_episodes_loading(false);
                    break;
                }
            }
        }

        if handle_download(&mut app, config, client.as_ref(), &tracker) {
            continue;
        }

        if handle_delete(&mut app, config, &tracker) {
            continue;
        }

        handle_playback_requests(&mut app, config, &playback_request_tx);
        drain_playback_request_queue(
            &mut app,
            &runtime_handle,
            &client,
            config,
            &tracker,
            &mut playback_request_rx,
            &playback_result_tx,
            &mut active_playback,
        );
        drain_playback_results(
            &mut app,
            &tracker,
            config,
            &mut playback_result_rx,
            &mut active_playback,
        );

        if app.should_quit() {
            break;
        }
    }

    if let Some(handle) = active_fetch.take() {
        handle.abort();
    }
    if let Some(handle) = active_playback.take() {
        handle.abort();
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

fn handle_search(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if !app.take_pending_search() {
        return;
    }

    if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
}

fn handle_filters(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    request_tx: &UnboundedSender<EpisodeFetchRequest>,
) {
    let mut apply_filter = false;

    if app.take_anime_selection_changed() {
        if let Some(anime_id) = app.current_anime_id() {
            app.set_episodes_loading(true);
            let generation = app.next_fetch_generation();
            let request = EpisodeFetchRequest {
                generation,
                anime_id,
            };
            if request_tx.send(request).is_err() {
                app.set_episodes_loading(false);
                app.set_details("Episode fetch queue unavailable.");
            } else {
                app.set_details("Fetching episodes...");
            }
        } else {
            app.clear_episodes();
            app.set_episodes_loading(false);
            app.set_details("Select an anime to load episodes.");
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

fn handle_download(
    app: &mut App,
    config: &AppConfig,
    client: &AnimeClient<FetchBackend>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
) -> bool {
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
        if let Err(err) = refresh_episode_indicators(app, tracker, config) {
            app.set_details(format!("Failed to refresh indicators: {err}"));
        }
        return true;
    }

    app.set_playback_status(PlaybackStatus::Downloading);
    let stream = match client.resolve_stream_url(&episode_id) {
        Ok(link) => link,
        Err(err) => {
            app.set_playback_status(PlaybackStatus::None);
            app.set_details(format!("Failed to resolve stream: {err}"));
            return true;
        }
    };

    let download_result = download_episode(config, &episode_id, &stream.url);
    app.set_playback_status(PlaybackStatus::None);

    match download_result {
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
            if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                app.set_details(format!("Failed to refresh indicators: {err}"));
            }
        }
        Err(err) => {
            app.set_details(format!("Download failed: {err}"));
        }
    }

    false
}

fn handle_delete(app: &mut App, config: &AppConfig, tracker: &Arc<Mutex<EpisodeTracker>>) -> bool {
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
            if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                app.set_details(format!("Failed to refresh indicators: {err}"));
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

fn handle_playback_requests(
    app: &mut App,
    config: &Arc<AppConfig>,
    playback_request_tx: &UnboundedSender<PlaybackRequest>,
) {
    if !app.take_pending_play_async() {
        return;
    }

    if app.playback_in_progress() {
        app.set_details("Playback already running");
        return;
    }

    let Some(episode_id) = app.current_episode_id() else {
        app.set_details("Highlight an episode to play");
        return;
    };

    let episode_title = app.current_episode_title();
    let using_local = local_playback_url(config, &episode_id).is_some();

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

    let request = PlaybackRequest {
        episode_id: episode_id.clone(),
        episode_title: episode_title.clone(),
    };

    if playback_request_tx.send(request).is_err() {
        app.set_details("Playback queue disconnected.");
        app.set_current_playing_episode(None);
        return;
    }

    app.set_current_playback_titles(app.current_anime_title(), episode_title.clone());

    app.set_current_playing_episode(Some(episode_id));
    app.set_playback_in_progress(true);
    app.set_playback_status(PlaybackStatus::Playing);
}

#[allow(clippy::too_many_arguments)]
fn drain_playback_request_queue(
    app: &mut App,
    runtime: &Handle,
    client: &Arc<AnimeClient<FetchBackend>>,
    config: &Arc<AppConfig>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    request_rx: &mut UnboundedReceiver<PlaybackRequest>,
    result_tx: &UnboundedSender<PlaybackResult>,
    active_playback: &mut Option<AbortHandle>,
) {
    loop {
        match request_rx.try_recv() {
            Ok(request) => {
                if let Some(handle) = active_playback.take() {
                    handle.abort();
                }
                let abort_handle = spawn_playback_task(
                    runtime,
                    Arc::clone(client),
                    Arc::clone(config),
                    Arc::clone(tracker),
                    request,
                    result_tx.clone(),
                );
                *active_playback = Some(abort_handle);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                app.set_details("Playback request queue disconnected.");
                app.set_playback_status(PlaybackStatus::None);
                app.set_playback_in_progress(false);
                app.set_current_playing_episode(None);
                break;
            }
        }
    }
}

fn drain_playback_results(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &Arc<AppConfig>,
    result_rx: &mut UnboundedReceiver<PlaybackResult>,
    active_playback: &mut Option<AbortHandle>,
) {
    loop {
        match result_rx.try_recv() {
            Ok(result) => {
                *active_playback = None;
                app.set_playback_status(PlaybackStatus::None);
                app.set_playback_in_progress(false);
                app.set_current_playing_episode(None);

                let PlaybackResult {
                    episode_title,
                    outcome,
                } = result;

                match outcome {
                    Ok(()) => {
                        if let Some(title) = episode_title {
                            app.set_details(format!("Finished playing {title}"));
                        } else {
                            app.set_details("Playback finished");
                        }
                    }
                    Err(err) => {
                        app.set_details(format!("Playback failed: {err}"));
                    }
                }

                if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                    app.set_details(format!("Failed to refresh indicators: {err}"));
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                app.set_details("Playback worker disconnected.");
                app.set_playback_status(PlaybackStatus::None);
                app.set_playback_in_progress(false);
                app.set_current_playing_episode(None);
                break;
            }
        }
    }
}

#[derive(Parser)]
struct Args {
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
}

fn apply_episode_filter(app: &mut App, tracker: &Arc<Mutex<EpisodeTracker>>) -> Result<()> {
    if let Some(filter) = app.current_filter() {
        let filtered = {
            let guard = tracker
                .lock()
                .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
            guard.filter_episodes(app.unfiltered_episodes(), filter)
        };
        app.set_filtered_episodes(filtered);
    } else {
        app.clear_filtered_episodes();
    }

    Ok(())
}

fn mark_episode_started(tracker: &Arc<Mutex<EpisodeTracker>>, episode_id: &str) -> Result<()> {
    let mut guard = tracker
        .lock()
        .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
    guard.mark_started(episode_id)?;
    Ok(())
}

fn refresh_episode_indicators(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
) -> Result<()> {
    let indicators = {
        let guard = tracker
            .lock()
            .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
        let mut indicators = HashMap::with_capacity(app.unfiltered_episodes().len());
        for episode in app.unfiltered_episodes() {
            let state = guard.state_for(&episode.id);
            let watched = state.as_ref().is_some_and(|status| status.watched);
            let in_progress = state.as_ref().is_some_and(|status| status.in_progress);
            let downloaded = episode_file_path(config, &episode.id).exists();
            indicators.insert(
                episode.id.clone(),
                EpisodeIndicators {
                    watched,
                    in_progress,
                    downloaded,
                },
            );
        }
        indicators
    };
    app.set_episode_indicators(indicators);
    Ok(())
}

fn update_playback_elapsed(app: &mut App, tracker: &Arc<Mutex<EpisodeTracker>>) {
    if let Some(episode_id) = app.current_playing_episode_id() {
        if let Ok(guard) = tracker.lock() {
            let elapsed = guard
                .progress_for(episode_id)
                .and_then(|progress| progress.last_position_sec);
            app.set_playback_elapsed(elapsed);
        } else {
            warn!("episode tracker lock poisoned while updating playback progress");
            app.set_playback_elapsed(None);
        }
    } else {
        app.set_playback_elapsed(None);
    }
}

fn spawn_episode_fetch_task(
    runtime: &Handle,
    client: Arc<AnimeClient<FetchBackend>>,
    request: EpisodeFetchRequest,
    result_tx: UnboundedSender<EpisodeFetchResult>,
) -> AbortHandle {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let EpisodeFetchRequest {
        generation,
        anime_id,
    } = request;

    runtime.spawn({
        let fut = Abortable::new(
            async move {
                sleep(Duration::from_millis(200)).await;
                let blocking_result =
                    tokio::task::spawn_blocking(move || client.list_episodes(&anime_id)).await;
                let result = match blocking_result {
                    Ok(res) => res,
                    Err(err) => Err(anyhow!("episode fetch join failed: {err}")),
                };
                let _ = result_tx.send(EpisodeFetchResult { generation, result });
            },
            abort_registration,
        );
        async move {
            let _ = fut.await;
        }
    });

    abort_handle
}

fn spawn_playback_task(
    runtime: &Handle,
    client: Arc<AnimeClient<FetchBackend>>,
    config: Arc<AppConfig>,
    tracker: Arc<Mutex<EpisodeTracker>>,
    request: PlaybackRequest,
    result_tx: UnboundedSender<PlaybackResult>,
) -> AbortHandle {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let fallback_title = request.episode_title.clone();

    runtime.spawn({
        let fut = Abortable::new(
            async move {
                let blocking_result = tokio::task::spawn_blocking(move || {
                    run_playback_job(&config, &tracker, &client, request)
                })
                .await;

                let playback_result = match blocking_result {
                    Ok(result) => result,
                    Err(err) => PlaybackResult {
                        episode_title: fallback_title,
                        outcome: Err(anyhow!("playback join failed: {err}")),
                    },
                };

                let _ = result_tx.send(playback_result);
            },
            abort_registration,
        );
        async move {
            let _ = fut.await;
        }
    });

    abort_handle
}

fn run_playback_job(
    config: &Arc<AppConfig>,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    client: &Arc<AnimeClient<FetchBackend>>,
    request: PlaybackRequest,
) -> PlaybackResult {
    let PlaybackRequest {
        episode_id,
        episode_title,
    } = request;

    let outcome: Result<()> = (|| {
        let (target_url, using_local) = if let Some(url) = local_playback_url(config, &episode_id) {
            (url.to_string(), true)
        } else {
            let stream = client.resolve_stream_url(&episode_id)?;
            (stream.url.to_string(), false)
        };

        info!(
            "playback requested for '{episode_id}' using {} source",
            if using_local { "local" } else { "remote" }
        );

        mark_episode_started(tracker, &episode_id)?;
        playback::play_episode(config, tracker, &episode_id, target_url.as_str())?;
        Ok(())
    })();

    PlaybackResult {
        episode_title,
        outcome,
    }
}
