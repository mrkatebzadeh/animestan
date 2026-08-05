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
mod browse;
mod cache;
mod events;
mod flow;
mod media;
mod tasks;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use animestan_core::{
    AniDbMetadataProvider, AnimeClient, AppConfig, EpisodeTracker, FavoriteStore, init_logging,
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
use tokio::sync::mpsc::unbounded_channel;

use crate::actions::{
    handle_delete, handle_download, handle_episode_mark_actions, handle_library_actions,
};
use crate::app::App;
use crate::bootstrap::{
    BackgroundRefreshHandles, initialize_app_state, start_background_refreshes,
};
use crate::browse::{handle_current_anime_refresh, handle_filters, handle_search};
use crate::cache::{CoverCache, EpisodeCache};
use crate::events::{Event, EventHandler};
use crate::flow::{
    drain_background_episode_refresh_results, drain_episode_fetch_requests,
    drain_episode_fetch_results, drain_playback_request_queue, drain_playback_results,
    handle_playback_requests, update_playback_elapsed,
};
use crate::media::{
    ActiveMetadataFetch, ImageLoadRequest, ImageLoadResult, drain_image_results,
    drain_metadata_results, handle_list_metadata_fetch, handle_metadata_fetch, spawn_image_loader,
};
use crate::tasks::{
    BackgroundEpisodeRefreshResult, EpisodeFetchRequest, EpisodeFetchResult, MetadataFetchResult,
    PlaybackRequest, PlaybackResult,
};
use crate::theme::Theme;

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Arc::new(AppConfig::load_default().context("failed to load configuration")?);
    let theme = Arc::new(Theme::load().context("failed to load theme configuration")?);
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
    let metadata_resolver = Arc::new(AniDbMetadataProvider::from_config(config.as_ref()));
    let (request_tx, mut request_rx) = unbounded_channel::<EpisodeFetchRequest>();
    let (result_tx, mut result_rx) = unbounded_channel::<EpisodeFetchResult>();
    let mut active_fetch: Option<AbortHandle> = None;
    let episode_cache = Arc::new(Mutex::new(EpisodeCache::load(config)));
    let (playback_request_tx, mut playback_request_rx) = unbounded_channel::<PlaybackRequest>();
    let (playback_result_tx, mut playback_result_rx) = unbounded_channel::<PlaybackResult>();
    let mut active_playback: Option<AbortHandle> = None;
    let (metadata_result_tx, mut metadata_result_rx) = unbounded_channel::<MetadataFetchResult>();
    let (background_job_tx, mut background_job_rx) =
        unbounded_channel::<BackgroundEpisodeRefreshResult>();
    let (image_request_tx, image_request_rx) = unbounded_channel::<ImageLoadRequest>();
    let (image_result_tx, mut image_result_rx) = unbounded_channel::<ImageLoadResult>();
    let mut active_metadata_fetch: Option<AbortHandle> = None;
    let mut active_list_metadata_fetch: Option<ActiveMetadataFetch> = None;
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

        handle_library_actions(&mut app, &mut favorites);

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
        handle_current_anime_refresh(
            &mut app,
            &runtime_handle,
            &metadata_resolver,
            &metadata_result_tx,
            &mut active_list_metadata_fetch,
            &request_tx,
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

        drain_background_episode_refresh_results(
            &mut app,
            &tracker,
            config,
            &mut background_job_rx,
        );

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

#[derive(Parser)]
struct Args {
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbosity: u8,
}
