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

use std::sync::Arc;

use animestan_core::{AnimeMetadata, AppConfig, MetadataResolver};
use futures::future::AbortHandle;
use image::DynamicImage;
use ratatui_image::Resize;
use reqwest::Client;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;

use crate::app::App;
use crate::cache::{CoverCache, save_metadata_cache_file};
use crate::tasks::{
    MetadataFetchRequest, MetadataFetchResult, MetadataTarget, spawn_metadata_fetch_task,
};

pub(crate) struct ImageLoadRequest {
    pub(crate) id: String,
    pub(crate) url: String,
}

pub(crate) struct ImageLoadResult {
    pub(crate) id: String,
    pub(crate) image: Option<DynamicImage>,
    pub(crate) error: Option<String>,
}

pub(crate) fn spawn_image_loader(
    runtime: &Handle,
    mut request_rx: UnboundedReceiver<ImageLoadRequest>,
    result_tx: UnboundedSender<ImageLoadResult>,
) {
    runtime.spawn({
        let client = Client::new();
        async move {
            let mut join_set = JoinSet::new();
            loop {
                tokio::select! {
                    Some(request) = request_rx.recv() => {
                        let client = client.clone();
                        join_set.spawn(async move {
                            let mut result = ImageLoadResult {
                                id: request.id,
                                image: None,
                                error: None,
                            };
                            let response = client.get(&request.url).send().await;
                            match response {
                                Ok(resp) => match resp.bytes().await {
                                    Ok(bytes) => match image::load_from_memory(&bytes) {
                                        Ok(image) => result.image = Some(image),
                                        Err(err) => result.error = Some(err.to_string()),
                                    },
                                    Err(err) => result.error = Some(err.to_string()),
                                },
                                Err(err) => result.error = Some(err.to_string()),
                            }
                            result
                        });
                    }
                    Some(result) = join_set.join_next() => {
                        if let Ok(result) = result {
                            let _ = result_tx.send(result);
                        }
                    }
                    else => break,
                }
            }
        }
    });
}

pub(crate) fn queue_image_load(
    app: &mut App,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    anime_id: &str,
    image_url: &str,
) {
    if !app.can_display_images() {
        return;
    }
    if app.image_pending(anime_id) {
        return;
    }
    if app.image_state().get_image_state(anime_id).is_some() {
        return;
    }
    match cover_cache.get(anime_id) {
        Ok(Some(image)) => {
            if !insert_cover_protocol(app, anime_id, image) {
                app.set_details("Cover cached but cannot render yet.");
            }
            return;
        }
        Ok(None) => {}
        Err(err) => {
            app.set_details(format!("Failed to load cached cover: {err}"));
            return;
        }
    }
    app.mark_image_pending(anime_id.to_string());
    if image_request_tx
        .send(ImageLoadRequest {
            id: anime_id.to_string(),
            url: image_url.to_string(),
        })
        .is_err()
    {
        app.clear_image_pending(anime_id);
    }
}

pub(crate) fn handle_metadata_fetch(
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
        MetadataTarget::Background | MetadataTarget::List | MetadataTarget::CurrentRefresh => {
            return;
        }
    };

    let generation = match target {
        MetadataTarget::InfoModal => app.next_info_fetch_generation(),
        MetadataTarget::SearchResults => app.next_search_results_metadata_generation(),
        MetadataTarget::List | MetadataTarget::Background | MetadataTarget::CurrentRefresh => 0,
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

pub(crate) fn handle_list_metadata_fetch(
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

pub(crate) fn drain_metadata_results(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    result_rx: &mut UnboundedReceiver<MetadataFetchResult>,
    active_list_metadata_fetch: &mut Option<AbortHandle>,
) {
    loop {
        match result_rx.try_recv() {
            Ok(fetch_result) => match fetch_result.target {
                MetadataTarget::InfoModal => handle_info_modal_metadata_result(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    fetch_result,
                ),
                MetadataTarget::SearchResults => handle_search_results_metadata_result(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    fetch_result,
                ),
                MetadataTarget::List => handle_list_metadata_result(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    fetch_result,
                    active_list_metadata_fetch,
                ),
                MetadataTarget::CurrentRefresh => handle_current_refresh_metadata_result(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    fetch_result,
                    active_list_metadata_fetch,
                ),
                MetadataTarget::Background => handle_background_metadata_result(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    fetch_result,
                ),
            },
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                app.set_info_modal_loading(false);
                app.set_info_modal_error("Metadata fetch worker disconnected.");
                break;
            }
        }
    }
}

fn handle_info_modal_metadata_result(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    fetch_result: MetadataFetchResult,
) {
    if fetch_result.generation != app.current_info_fetch_generation() {
        return;
    }
    app.set_info_modal_loading(false);
    match fetch_result.result {
        Ok(metadata) => {
            let title = metadata.title.clone();
            if let Some(anime_id) = fetch_result.anime_id.clone() {
                store_metadata_side_effects(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    &anime_id,
                    &metadata,
                );
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

fn handle_search_results_metadata_result(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    fetch_result: MetadataFetchResult,
) {
    if fetch_result.generation != app.current_search_results_metadata_generation() {
        return;
    }
    match fetch_result.result {
        Ok(metadata) => {
            let title = metadata.title.clone();
            if let Some(anime_id) = fetch_result.anime_id.clone() {
                store_metadata_side_effects(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    &anime_id,
                    &metadata,
                );
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

fn handle_list_metadata_result(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    fetch_result: MetadataFetchResult,
    active_list_metadata_fetch: &mut Option<AbortHandle>,
) {
    *active_list_metadata_fetch = None;
    if let Some(anime_id) = fetch_result.anime_id {
        match fetch_result.result {
            Ok(metadata) => {
                store_metadata_side_effects(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    &anime_id,
                    &metadata,
                );
            }
            Err(_) => {
                app.set_metadata_failure(&anime_id);
            }
        }
    }
}

fn handle_background_metadata_result(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    fetch_result: MetadataFetchResult,
) {
    if let Some(anime_id) = fetch_result.anime_id {
        match fetch_result.result {
            Ok(metadata) => {
                store_metadata_side_effects(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    &anime_id,
                    &metadata,
                );
            }
            Err(_) => {
                app.set_metadata_failure(&anime_id);
            }
        }
    }
    app.finish_metadata_background_fetch();
}

fn handle_current_refresh_metadata_result(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    fetch_result: MetadataFetchResult,
    active_list_metadata_fetch: &mut Option<AbortHandle>,
) {
    *active_list_metadata_fetch = None;
    if fetch_result.generation != app.current_manual_metadata_generation() {
        return;
    }

    if let Some(anime_id) = fetch_result.anime_id {
        match fetch_result.result {
            Ok(metadata) => {
                let title = metadata.title.clone();
                store_metadata_side_effects(
                    app,
                    config,
                    cover_cache,
                    image_request_tx,
                    &anime_id,
                    &metadata,
                );
                app.set_details(format!("Refreshed metadata for {title}"));
            }
            Err(err) => {
                app.set_metadata_failure(&anime_id);
                app.set_details(format!("Metadata refresh failed: {err}"));
            }
        }
    }
}

pub(crate) fn drain_image_results(
    app: &mut App,
    cover_cache: &mut CoverCache,
    result_rx: &mut UnboundedReceiver<ImageLoadResult>,
) {
    while let Ok(result) = result_rx.try_recv() {
        app.clear_image_pending(&result.id);
        if let Some(image) = result.image {
            if let Err(err) = cover_cache.insert(&result.id, &image) {
                app.set_details(format!("Failed to cache cover: {err}"));
            }
            if !insert_cover_protocol(app, &result.id, image) {
                app.set_details("Cover cached but cannot render yet.");
            }
        } else if let Some(error) = result.error {
            app.set_details(format!("Failed to load cover: {error}"));
        }
    }
}

fn insert_cover_protocol(app: &mut App, anime_id: &str, image: DynamicImage) -> bool {
    let area = app.image_state().area();
    if area.width == 0 || area.height == 0 {
        return false;
    }
    let protocol = app
        .image_picker_mut()
        .and_then(|picker| picker.new_protocol(image, area, Resize::Fit(None)).ok());
    if let Some(protocol) = protocol {
        app.image_state_mut()
            .insert_manga(anime_id.to_string(), protocol);
        true
    } else {
        false
    }
}

fn store_metadata_side_effects(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    anime_id: &str,
    metadata: &AnimeMetadata,
) {
    app.store_metadata(anime_id, metadata);
    if let Err(err) = save_metadata_cache_file(config, anime_id, metadata) {
        app.set_details(format!("Failed to persist metadata cache: {err}"));
    }
    if let Some(image_url) = metadata.image_url.as_deref() {
        queue_image_load(app, cover_cache, image_request_tx, anime_id, image_url);
    }
}
