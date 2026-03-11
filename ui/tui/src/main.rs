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
mod theme;
mod ui;

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use animestan_core::{
    AnimeClient, AnimeMetadata, AppConfig, CoreResult, Episode, EpisodeTracker, FavoriteStore,
    FetchBackend, MetadataProvider, MetadataResolver, delete_episode, download_episode,
    episode_file_path, init_logging, local_playback_url,
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

use crate::app::{AnimeProgress, App, EpisodeIndicators, EpisodeMarkAction, PlaybackStatus};
use crate::events::{Event, EventHandler};
use crate::theme::Theme;

struct EpisodeFetchRequest {
    generation: u64,
    anime_id: String,
}

struct EpisodeFetchResult {
    generation: u64,
    anime_id: String,
    result: CoreResult<Vec<Episode>>,
}

struct EpisodeCache {
    dir: PathBuf,
}

impl EpisodeCache {
    fn load(config: &AppConfig) -> Self {
        let dir = config.episodes_cache_path().with_file_name("episodes");
        Self { dir }
    }

    fn get(&self, anime_id: &str) -> Option<Vec<Episode>> {
        let path = self.dir.join(format!("{anime_id}.json"));
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str::<Vec<Episode>>(&contents).ok())
    }

    fn insert(&mut self, anime_id: &str, episodes: &[Episode]) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let payload = serde_json::to_string_pretty(episodes)?;
        let path = self.dir.join(format!("{anime_id}.json"));
        std::fs::write(path, payload)?;
        Ok(())
    }
}

fn metadata_cache_dir(config: &AppConfig) -> PathBuf {
    config.metadata_cache_path().with_file_name("metadata")
}

fn metadata_cache_file(config: &AppConfig, anime_id: &str) -> PathBuf {
    metadata_cache_dir(config).join(format!("{anime_id}.json"))
}

fn load_metadata_cache_files(config: &AppConfig) -> HashMap<String, AnimeMetadata> {
    let mut entries = HashMap::new();
    let dir = metadata_cache_dir(config);
    let Ok(read_dir) = std::fs::read_dir(&dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(metadata) = serde_json::from_str::<AnimeMetadata>(&contents) {
                entries.insert(stem.to_string(), metadata);
            }
        }
    }
    entries
}

fn save_metadata_cache_file(
    config: &AppConfig,
    anime_id: &str,
    metadata: &AnimeMetadata,
) -> Result<()> {
    let dir = metadata_cache_dir(config);
    std::fs::create_dir_all(&dir)?;
    let path = metadata_cache_file(config, anime_id);
    let payload = serde_json::to_string_pretty(metadata)?;
    std::fs::write(path, payload)?;
    Ok(())
}

fn populate_anime_progress_from_cache(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    cache: &EpisodeCache,
) {
    if let Ok(guard) = tracker.lock() {
        let bookmark_ids: Vec<String> = app
            .bookmark_entries()
            .iter()
            .map(|entry| entry.anime.id.clone())
            .collect();
        for anime_id in bookmark_ids {
            if let Some(episodes) = cache.get(&anime_id) {
                let watched = episodes
                    .iter()
                    .filter(|episode| {
                        guard
                            .state_for(&episode.id)
                            .as_ref()
                            .is_some_and(|state| state.watched)
                    })
                    .count();
                app.set_anime_progress(
                    anime_id.clone(),
                    AnimeProgress {
                        watched,
                        total: episodes.len(),
                    },
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetadataTarget {
    InfoModal,
    SearchResults,
    List,
    Background,
}

struct MetadataFetchRequest {
    generation: u64,
    query: String,
    source_id: Option<String>,
    anime_id: Option<String>,
    target: MetadataTarget,
    force_refresh: bool,
}

struct MetadataFetchResult {
    generation: u64,
    target: MetadataTarget,
    anime_id: Option<String>,
    result: Result<AnimeMetadata, anyhow::Error>,
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
    let theme = Arc::new(Theme::load(&config).context("failed to load theme configuration")?);
    init_logging("animestan-tui", args.verbosity, &config, false)
        .context("failed to initialize logging")?;
    info!("launching animestan-tui");
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
    let mut active_metadata_fetch: Option<AbortHandle> = None;
    let mut active_list_metadata_fetch: Option<AbortHandle> = None;

    let mut app = App::new();
    for (anime_id, metadata) in load_metadata_cache_files(config) {
        app.store_metadata(&anime_id, &metadata);
    }
    app.sync_bookmark_cache(&favorites);
    if let Ok(cache_guard) = episode_cache.lock() {
        populate_anime_progress_from_cache(&mut app, &tracker, &cache_guard);
    }
    initialize_app(&mut app, client.as_ref());
    if let Err(err) = refresh_episode_indicators(&mut app, &tracker, config) {
        app.set_details(format!("Failed to refresh indicators: {err}"));
    }

    let background_metadata_targets: Vec<(String, String)> = app
        .bookmark_entries()
        .iter()
        .map(|entry| (entry.anime.id.clone(), entry.anime.title.clone()))
        .collect();
    let background_episode_targets: Vec<String> = app
        .bookmark_entries()
        .iter()
        .map(|entry| entry.anime.id.clone())
        .collect();
    let total_background_jobs =
        background_metadata_targets.len() + background_episode_targets.len();
    if total_background_jobs > 0 {
        app.start_metadata_background_refresh(total_background_jobs);
    }
    if !background_episode_targets.is_empty() {
        for anime_id in &background_episode_targets {
            app.mark_episode_refresh_pending(anime_id.clone());
        }
    }
    let background_metadata_handles = if background_metadata_targets.is_empty() {
        Vec::new()
    } else {
        spawn_background_metadata_refresh_tasks(
            &runtime_handle,
            &metadata_resolver,
            background_metadata_targets,
            &metadata_result_tx,
        )
    };
    let background_episode_handles = if background_episode_targets.is_empty() {
        Vec::new()
    } else {
        spawn_background_episode_refresh_tasks(
            &runtime_handle,
            &client,
            background_episode_targets,
            &episode_cache,
            background_job_tx.clone(),
        )
    };

    let mut events = EventHandler::new(Duration::from_millis(250));

    loop {
        update_playback_elapsed(&mut app, &tracker);
        terminal.draw(|frame| ui::render(frame, &mut app, theme))?;

        match events.next()? {
            Event::Input(key_event) => app.on_key(key_event),
            Event::Tick => app.advance_metadata_spinner(),
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
        handle_filters(&mut app, &tracker, config, &request_tx, &episode_cache);
        handle_metadata_fetch(
            &mut app,
            &metadata_resolver,
            &runtime_handle,
            &metadata_result_tx,
            &mut active_metadata_fetch,
        );

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
                            let anime_id = fetch_result.anime_id.clone();
                            app.set_episodes(episodes.clone());
                            if let Err(err) =
                                episode_cache.lock().unwrap().insert(&anime_id, &episodes)
                            {
                                app.set_details(format!("Failed to cache episodes: {err}"));
                            }
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
                            app.clear_episode_refresh_pending(&anime_id);
                        }
                        Err(err) => {
                            app.set_episodes_loading(false);
                            app.set_details(format!("Episode load failed: {err}"));
                            app.clear_episode_refresh_pending(&fetch_result.anime_id);
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

        loop {
            match metadata_result_rx.try_recv() {
                Ok(fetch_result) => match fetch_result.target {
                    MetadataTarget::InfoModal => {
                        if fetch_result.generation != app.current_info_fetch_generation() {
                            continue;
                        }
                        app.set_info_modal_loading(false);
                        match fetch_result.result {
                            Ok(metadata) => {
                                let title = metadata.title.clone();
                                if let Some(anime_id) = fetch_result.anime_id.clone() {
                                    app.store_metadata(&anime_id, &metadata);
                                    if let Err(err) =
                                        save_metadata_cache_file(config, &anime_id, &metadata)
                                    {
                                        app.set_details(format!(
                                            "Failed to persist metadata cache: {err}"
                                        ));
                                    }
                                }
                                app.set_info_modal_metadata(metadata);
                                app.set_details(format!("Loaded metadata for {title}"));
                            }
                            Err(err) => {
                                let message = format!("Metadata load failed: {err}");
                                app.set_info_modal_error(message.clone());
                                app.set_details(message);
                            }
                        }
                    }
                    MetadataTarget::SearchResults => {
                        if fetch_result.generation
                            != app.current_search_results_metadata_generation()
                        {
                            continue;
                        }
                        match fetch_result.result {
                            Ok(metadata) => {
                                let title = metadata.title.clone();
                                if let Some(anime_id) = fetch_result.anime_id.clone() {
                                    app.store_metadata(&anime_id, &metadata);
                                    if let Err(err) =
                                        save_metadata_cache_file(config, &anime_id, &metadata)
                                    {
                                        app.set_details(format!(
                                            "Failed to persist metadata cache: {err}"
                                        ));
                                    }
                                }
                                app.set_search_results_metadata(metadata);
                                app.set_details(format!("Loaded metadata for {title}"));
                            }
                            Err(err) => {
                                let message = format!("Metadata load failed: {err}");
                                app.set_search_results_metadata_error(message.clone());
                                app.set_details(message);
                            }
                        }
                    }
                    MetadataTarget::List => {
                        active_list_metadata_fetch = None;
                        if let Some(anime_id) = fetch_result.anime_id {
                            match fetch_result.result {
                                Ok(metadata) => {
                                    app.store_metadata(&anime_id, &metadata);
                                    if let Err(err) =
                                        save_metadata_cache_file(config, &anime_id, &metadata)
                                    {
                                        app.set_details(format!(
                                            "Failed to persist metadata cache: {err}"
                                        ));
                                    }
                                }
                                Err(_) => {
                                    app.set_metadata_failure(&anime_id);
                                }
                            }
                        }
                    }
                    MetadataTarget::Background => {
                        if let Some(anime_id) = fetch_result.anime_id {
                            match fetch_result.result {
                                Ok(metadata) => {
                                    app.store_metadata(&anime_id, &metadata);
                                    if let Err(err) =
                                        save_metadata_cache_file(config, &anime_id, &metadata)
                                    {
                                        app.set_details(format!(
                                            "Failed to persist metadata cache: {err}"
                                        ));
                                    }
                                }
                                Err(_) => {
                                    app.set_metadata_failure(&anime_id);
                                }
                            }
                        }
                        app.finish_metadata_background_fetch();
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    app.set_info_modal_loading(false);
                    app.set_info_modal_error("Metadata fetch worker disconnected.");
                    break;
                }
            }
        }

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

fn initialize_app(app: &mut App, client: &AnimeClient<FetchBackend>) {
    if app.search_query().trim().is_empty() {
        app.set_details("Press / to search for an anime.");
    } else if let Err(err) = app.search(client) {
        app.set_details(format!("Search failed: {err}"));
    }
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
) {
    let mut apply_filter = false;

    if app.take_anime_selection_changed() {
        if let Some(anime_id) = app.current_anime_id() {
            app.record_anime_history(&anime_id);
            let cached = {
                let cache = episode_cache.lock().unwrap();
                cache.get(&anime_id)
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

fn handle_metadata_fetch(
    app: &mut App,
    resolver: &Arc<MetadataResolver>,
    runtime: &Handle,
    result_tx: &UnboundedSender<MetadataFetchResult>,
    active_fetch: &mut Option<AbortHandle>,
) {
    let target = if app.take_pending_info_fetch() {
        MetadataTarget::InfoModal
    } else if app.take_pending_search_results_metadata_fetch() {
        MetadataTarget::SearchResults
    } else {
        return;
    };

    let (query, source_id, anime_id) = match target {
        MetadataTarget::InfoModal => {
            let Some(query) = app.current_anime_title() else {
                app.set_info_modal_error("Highlight an anime to view metadata.");
                app.set_info_modal_loading(false);
                return;
            };
            let source_id = app.current_anime_id();
            app.set_info_modal_loading(true);
            app.set_details(format!("Fetching metadata for {query}..."));
            (query, source_id.clone(), source_id)
        }
        MetadataTarget::SearchResults => {
            let Some(result) = app.current_search_result() else {
                app.set_search_results_metadata_error("Highlight an anime to view metadata.");
                app.set_details("Highlight an anime to view metadata.");
                return;
            };
            let query_string = result.title.clone();
            let source_id = Some(result.id.clone());
            app.set_details(format!("Fetching metadata for {query_string}..."));
            (query_string, source_id.clone(), source_id)
        }
        MetadataTarget::Background | MetadataTarget::List => return,
    };

    let generation = match target {
        MetadataTarget::InfoModal => app.next_info_fetch_generation(),
        MetadataTarget::SearchResults => app.next_search_results_metadata_generation(),
        MetadataTarget::List | MetadataTarget::Background => 0,
    };

    if let Some(handle) = active_fetch.take() {
        handle.abort();
    }

    let request = MetadataFetchRequest {
        generation,
        query,
        source_id,
        anime_id,
        target,
        force_refresh: false,
    };
    let abort_handle =
        spawn_metadata_fetch_task(runtime, Arc::clone(resolver), request, result_tx.clone());
    *active_fetch = Some(abort_handle);
}

fn handle_list_metadata_fetch(
    app: &mut App,
    resolver: &Arc<MetadataResolver>,
    runtime: &Handle,
    result_tx: &UnboundedSender<MetadataFetchResult>,
    active_fetch: &mut Option<AbortHandle>,
) {
    if active_fetch.is_some() {
        return;
    }

    let Some((anime_id, title)) = app.next_metadata_fetch_candidate() else {
        return;
    };

    let request = MetadataFetchRequest {
        generation: 0,
        query: title,
        source_id: Some(anime_id.clone()),
        anime_id: Some(anime_id),
        target: MetadataTarget::List,
        force_refresh: false,
    };

    let abort_handle =
        spawn_metadata_fetch_task(runtime, Arc::clone(resolver), request, result_tx.clone());
    *active_fetch = Some(abort_handle);
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

fn handle_episode_mark_actions(
    app: &mut App,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    config: &AppConfig,
) {
    let Some(action) = app.take_pending_episode_mark_action() else {
        return;
    };

    let mark_result = {
        let Ok(mut guard) = tracker.lock() else {
            app.set_details("Episode tracker lock poisoned.");
            return;
        };
        perform_episode_mark_action(app, &mut guard, action)
    };

    let message = match mark_result {
        Ok(message) => message,
        Err(err) => {
            app.set_details(err);
            return;
        }
    };

    if let Err(err) = refresh_episode_indicators(app, tracker, config) {
        app.set_details(format!("Failed to refresh indicators: {err}"));
    } else {
        app.set_details(message);
    }
}

fn perform_episode_mark_action(
    app: &App,
    tracker: &mut EpisodeTracker,
    action: EpisodeMarkAction,
) -> Result<String, String> {
    match action {
        EpisodeMarkAction::Current { watched } => {
            let episode_id = app
                .current_episode_id()
                .ok_or_else(|| "Highlight an episode to mark it.".to_string())?;
            let result = if watched {
                tracker.mark_watched(&episode_id)
            } else {
                tracker.mark_unwatched(&episode_id)
            };
            result.map_err(|err| err.to_string())?;
            let message = if watched {
                "Marked current episode as watched."
            } else {
                "Marked current episode as unwatched."
            };
            Ok(message.to_string())
        }
        EpisodeMarkAction::All { watched } => {
            let episodes = app.unfiltered_episodes();
            if episodes.is_empty() {
                return Err("No episodes loaded to mark.".to_string());
            }
            let ids: Vec<String> = episodes.iter().map(|episode| episode.id.clone()).collect();
            tracker
                .mark_many(&ids, watched)
                .map_err(|err| err.to_string())?;
            let message = if watched {
                "Marked all loaded episodes as watched."
            } else {
                "Marked all loaded episodes as unwatched."
            };
            Ok(message.to_string())
        }
        EpisodeMarkAction::UpToCurrent => {
            let current_id = app
                .current_episode_id()
                .ok_or_else(|| "Highlight an episode to set the range.".to_string())?;
            let mut episodes: Vec<Episode> = app.unfiltered_episodes().to_vec();
            if episodes.is_empty() {
                return Err("No episodes loaded to mark.".to_string());
            }
            episodes.sort_by(|a, b| a.number.cmp(&b.number));
            let mut ids = Vec::new();
            for episode in episodes {
                ids.push(episode.id.clone());
                if episode.id == current_id {
                    break;
                }
            }
            if ids.is_empty() || ids.last() != Some(&current_id) {
                return Err("Current episode is not present in the loaded list.".to_string());
            }
            tracker
                .mark_many(&ids, true)
                .map_err(|err| err.to_string())?;
            Ok("Marked episodes up to current as watched.".to_string())
        }
    }
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

    let (episode_id, episode_title, anime_id) =
        if let Some((episode_id, episode_title, anime_id)) = app.take_pending_playback_override() {
            (episode_id, episode_title, anime_id)
        } else {
            let Some(episode_id) = app.current_episode_id() else {
                app.set_details("Highlight an episode to play");
                return;
            };
            let anime_id = app.current_anime_id();
            let episode_title = app.current_episode_title();
            (episode_id, episode_title, anime_id)
        };
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
    let requested_title = request.episode_title.clone();

    if playback_request_tx.send(request).is_err() {
        app.set_details("Playback queue disconnected.");
        app.set_current_playing_episode(None);
        return;
    }

    app.record_played_episode(episode_id.clone(), anime_id, requested_title);
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
    app.record_selected_anime_progress();
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
        let fetch_id = anime_id.clone();
        let fut = Abortable::new(
            async move {
                sleep(Duration::from_millis(200)).await;
                let blocking_result =
                    tokio::task::spawn_blocking(move || client.list_episodes(&fetch_id)).await;
                let result = match blocking_result {
                    Ok(res) => res,
                    Err(err) => Err(anyhow!("episode fetch join failed: {err}")),
                };
                let _ = result_tx.send(EpisodeFetchResult {
                    generation,
                    anime_id,
                    result,
                });
            },
            abort_registration,
        );
        async move {
            let _ = fut.await;
        }
    });

    abort_handle
}

fn spawn_metadata_fetch_task(
    runtime: &Handle,
    resolver: Arc<MetadataResolver>,
    request: MetadataFetchRequest,
    result_tx: UnboundedSender<MetadataFetchResult>,
) -> AbortHandle {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let MetadataFetchRequest {
        generation,
        query,
        source_id,
        anime_id,
        target,
        force_refresh,
    } = request;

    runtime.spawn({
        let fut = Abortable::new(
            async move {
                let blocking_result = tokio::task::spawn_blocking(move || {
                    if force_refresh {
                        if let Some(id) = source_id.as_deref() {
                            resolver.refresh_by_id(id, &query)
                        } else {
                            resolver.refresh_by_query(&query)
                        }
                    } else if let Some(id) = source_id.as_deref() {
                        resolver.fetch_by_id(id, &query)
                    } else {
                        resolver.fetch_by_query(&query)
                    }
                })
                .await;
                let result = match blocking_result {
                    Ok(inner) => inner.map_err(|err| anyhow!("metadata fetch failed: {err}")),
                    Err(err) => Err(anyhow!("metadata fetch join failed: {err}")),
                };
                let _ = result_tx.send(MetadataFetchResult {
                    generation,
                    target,
                    anime_id,
                    result,
                });
            },
            abort_registration,
        );
        async move {
            let _ = fut.await;
        }
    });

    abort_handle
}

#[allow(clippy::needless_pass_by_value)]
fn spawn_background_episode_refresh_tasks(
    runtime: &Handle,
    client: &Arc<AnimeClient<FetchBackend>>,
    anime_ids: Vec<String>,
    cache: &Arc<Mutex<EpisodeCache>>,
    background_job_tx: UnboundedSender<()>,
) -> Vec<AbortHandle> {
    anime_ids
        .into_iter()
        .filter(|anime_id| !anime_id.is_empty())
        .map(|anime_id| {
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let client = Arc::clone(client);
            let cache = Arc::clone(cache);
            let fetch_id = anime_id.clone();
            let job_tx = background_job_tx.clone();
            runtime.spawn({
                let fut = Abortable::new(
                    async move {
                        let blocking_result =
                            tokio::task::spawn_blocking(move || client.list_episodes(&fetch_id))
                                .await;
                        if let Ok(Ok(episodes)) = blocking_result {
                            let _ = cache.lock().unwrap().insert(&anime_id, &episodes);
                            let _ = job_tx.send(());
                        }
                    },
                    abort_registration,
                );
                async move {
                    let _ = fut.await;
                }
            });
            abort_handle
        })
        .collect()
}

fn spawn_background_metadata_refresh_tasks(
    runtime: &Handle,
    resolver: &Arc<MetadataResolver>,
    entries: Vec<(String, String)>,
    result_tx: &UnboundedSender<MetadataFetchResult>,
) -> Vec<AbortHandle> {
    entries
        .into_iter()
        .map(|(anime_id, query)| {
            let request = MetadataFetchRequest {
                generation: 0,
                query,
                source_id: None,
                anime_id: Some(anime_id.clone()),
                target: MetadataTarget::Background,
                force_refresh: true,
            };
            spawn_metadata_fetch_task(runtime, Arc::clone(resolver), request, result_tx.clone())
        })
        .collect()
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
