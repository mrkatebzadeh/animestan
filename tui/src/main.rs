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

mod actions;
mod app;
mod bootstrap;
mod cache;
mod events;
mod flow;
mod media;
mod playback;
mod tasks;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use animestan_core::{
    AnimeClient, AppConfig, EpisodeTracker, FavoriteStore, FetchBackend, MetadataResolver,
    init_logging,
};
use anyhow::{Context, Result};
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::future::AbortHandle;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui_image::picker::Picker;
use spdlog::prelude::*;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::actions::{handle_delete, handle_download, handle_episode_mark_actions};
use crate::app::App;
use crate::bootstrap::{
    BackgroundRefreshHandles, initialize_app_state, start_background_refreshes,
};
use crate::cache::{CoverCache, EpisodeCache, cached_episodes};
use crate::events::{Event, EventHandler};
use crate::flow::{
    apply_episode_filter, drain_episode_fetch_requests, drain_episode_fetch_results,
    drain_playback_request_queue, drain_playback_results, handle_playback_requests,
    refresh_episode_indicators, update_playback_elapsed,
};
use crate::media::{
    ImageLoadRequest, ImageLoadResult, drain_image_results, drain_metadata_results,
    handle_list_metadata_fetch, handle_metadata_fetch, queue_image_load, spawn_image_loader,
};
use crate::tasks::{
    EpisodeFetchRequest, EpisodeFetchResult, MetadataFetchResult, PlaybackRequest, PlaybackResult,
};
use crate::theme::Theme;

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Arc::new(AppConfig::load_default().context("failed to load configuration")?);
    let theme = Arc::new(Theme::load(&config).context("failed to load theme configuration")?);
    init_logging("animestan", args.verbosity, &config, false)
        .context("failed to initialize logging")?;
    info!("launching animestan");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    run_app(&mut terminal, &config, &theme)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: &Arc<AppConfig>,
    theme: &Arc<Theme>,
) -> Result<()> {
    let tracker = Arc::new(Mutex::new(
        EpisodeTracker::load_default(config).context("failed to load episode tracker")?,
    ));
    let mut favorites = FavoriteStore::load_default(config).context("failed to load favorites")?;
    let client = Arc::new(AnimeClient::from_config(config.as_ref())?);
    let runtime = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    let runtime_handle = runtime.handle().clone();
    let metadata_resolver = Arc::new(MetadataResolver::from_config(config.as_ref()));
    let (request_tx, mut request_rx) = unbounded_channel::<EpisodeFetchRequest>();
    let (result_tx, mut result_rx) = unbounded_channel::<EpisodeFetchResult>();
    let mut active_fetch: Option<AbortHandle> = None;
    let episode_cache = Arc::new(Mutex::new(EpisodeCache::load(config)));
    let (playback_request_tx, mut playback_request_rx) = unbounded_channel::<PlaybackRequest>();
    let (playback_result_tx, mut playback_result_rx) = unbounded_channel::<PlaybackResult>();
    let mut active_playback: Option<AbortHandle> = None;
    let (metadata_result_tx, mut metadata_result_rx) = unbounded_channel::<MetadataFetchResult>();
    let (background_job_tx, mut background_job_rx) = unbounded_channel::<()>();
    let (image_request_tx, image_request_rx) = unbounded_channel::<ImageLoadRequest>();
    let (image_result_tx, mut image_result_rx) = unbounded_channel::<ImageLoadResult>();
    let mut active_metadata_fetch: Option<AbortHandle> = None;
    let mut active_list_metadata_fetch: Option<AbortHandle> = None;
    let mut cover_cache = CoverCache::load(config);

    let mut app = App::new();
    let picker = Picker::from_query_stdio().ok();
    app.set_image_picker(picker);
    initialize_app_state(
        &mut app,
        client.as_ref(),
        &favorites,
        &episode_cache,
        &tracker,
        config,
    );
    let BackgroundRefreshHandles {
        metadata: background_metadata_handles,
        episode: background_episode_handles,
    } = start_background_refreshes(
        &mut app,
        &runtime_handle,
        &metadata_resolver,
        &client,
        &episode_cache,
        &background_job_tx,
        &metadata_result_tx,
    );

    spawn_image_loader(&runtime_handle, image_request_rx, image_result_tx);

    let mut events = EventHandler::new(Duration::from_millis(250));

    loop {
        update_playback_elapsed(&mut app, &tracker);
        terminal.draw(|frame| ui::render(frame, &mut app, theme))?;

        match events.next()? {
            Event::Input(key_event) => app.on_key(key_event),
            Event::Tick => {
                app.advance_metadata_throbber();
                app.image_state_mut().throbber_mut().calc_next();
            }
        }

        if app.take_pending_bookmark_toggle() {
            match app.toggle_bookmark(&mut favorites) {
                Ok(()) => {
                    let details = app.details().to_string();
                    app.load_bookmarks(&favorites);
                    app.set_details(details);
                }
                Err(err) => {
                    app.set_details(format!("Bookmark toggle failed: {err}"));
                }
            }
        }

        if app.take_pending_search_results_add() {
            if let Err(err) = app.add_current_search_result_to_bookmarks(&mut favorites) {
                app.set_details(format!("Failed to add anime to panel: {err}"));
            }
        }

        handle_search(&mut app, client.as_ref());
        handle_filters(
            &mut app,
            &tracker,
            config,
            &request_tx,
            &episode_cache,
            &mut cover_cache,
            &image_request_tx,
        );
        handle_metadata_fetch(
            &mut app,
            &metadata_resolver,
            &runtime_handle,
            &metadata_result_tx,
            &mut active_metadata_fetch,
        );

        drain_episode_fetch_requests(
            &mut app,
            &runtime_handle,
            &client,
            &mut request_rx,
            &result_tx,
            &mut active_fetch,
        );

        drain_episode_fetch_results(&mut app, &tracker, config, &episode_cache, &mut result_rx);

        drain_metadata_results(
            &mut app,
            config,
            &mut cover_cache,
            &image_request_tx,
            &mut metadata_result_rx,
            &mut active_list_metadata_fetch,
        );

        drain_image_results(&mut app, &mut cover_cache, &mut image_result_rx);

        while background_job_rx.try_recv().is_ok() {
            app.finish_metadata_background_fetch();
        }

        handle_list_metadata_fetch(
            &mut app,
            &metadata_resolver,
            &runtime_handle,
            &metadata_result_tx,
            &mut active_list_metadata_fetch,
        );

        if handle_download(&mut app, config, client.as_ref(), &tracker) {
            continue;
        }

        if handle_delete(&mut app, config, &tracker) {
            continue;
        }

        handle_episode_mark_actions(&mut app, &tracker, config);

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
    for handle in background_metadata_handles {
        handle.abort();
    }
    for handle in background_episode_handles {
        handle.abort();
    }

    Ok(())
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
    config: &AppConfig,
    request_tx: &UnboundedSender<EpisodeFetchRequest>,
    episode_cache: &Arc<Mutex<EpisodeCache>>,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
) {
    let mut apply_filter = false;

    if app.take_anime_selection_changed() {
        if let Some(anime_id) = app.current_anime_id() {
            app.record_anime_history(&anime_id);
            let cached = match cached_episodes(episode_cache, &anime_id) {
                Ok(cached) => cached,
                Err(err) => {
                    app.set_details(format!("Failed to access cached episodes: {err}"));
                    None
                }
            };
            let has_cached = cached.is_some();
            if let Some(cached) = cached {
                app.set_episodes(cached);
                app.set_details("Loaded cached episodes; refreshing...");
                if let Err(err) = refresh_episode_indicators(app, tracker, config) {
                    app.set_details(format!("Failed to refresh indicators: {err}"));
                }
            }
            let should_fetch = !app.episode_refresh_pending(&anime_id);
            if should_fetch {
                app.set_episodes_loading(true);
                app.mark_episode_refresh_pending(anime_id.clone());
                let generation = app.next_fetch_generation();
                let request = EpisodeFetchRequest {
                    generation,
                    anime_id: anime_id.clone(),
                };
                if request_tx.send(request).is_err() {
                    app.set_episodes_loading(false);
                    app.clear_episode_refresh_pending(&anime_id);
                    app.set_details("Episode fetch queue unavailable.");
                } else if has_cached {
                    app.set_details("Refreshing cached episodes...");
                } else {
                    app.set_details("Fetching episodes...");
                }
            }
            let image_url = app
                .cached_metadata_for_current_anime()
                .and_then(|metadata| metadata.image_url.clone());
            if let Some(image_url) = image_url {
                queue_image_load(app, cover_cache, image_request_tx, &anime_id, &image_url);
            }
            app.request_info_metadata();
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

#[derive(Parser)]
struct Args {
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
}
