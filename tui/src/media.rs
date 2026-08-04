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

use animestan_core::{AnimeMetadata, AppConfig, MetadataResolver, validate_media_url};
use futures::future::AbortHandle;
use image::DynamicImage;
use ratatui_image::Resize;
use reqwest::redirect::Policy;
use reqwest::{Client, Url};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;

use crate::app::App;
use crate::cache::{CoverCache, save_metadata_cache_file};
use crate::tasks::{
    MetadataFetchRequest, MetadataFetchResult, MetadataTarget, spawn_metadata_fetch_task,
};

pub(crate) struct ActiveMetadataFetch {
    pub(crate) anime_id: Option<String>,
    pub(crate) target: MetadataTarget,
    pub(crate) handle: AbortHandle,
}

pub(crate) struct ImageLoadRequest {
    pub(crate) id: String,
    pub(crate) url: String,
}

pub(crate) struct ImageLoadResult {
    pub(crate) id: String,
    pub(crate) image: Option<DynamicImage>,
    pub(crate) error: Option<String>,
}

fn image_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .build()
        .expect("image HTTP client should build")
}

fn validate_image_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|error| error.to_string())?;
    validate_media_url(&url).map_err(|error| error.to_string())?;
    Ok(url)
}

pub(crate) fn spawn_image_loader(
    runtime: &Handle,
    mut request_rx: UnboundedReceiver<ImageLoadRequest>,
    result_tx: UnboundedSender<ImageLoadResult>,
) {
    runtime.spawn({
        let client = image_client();
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
                            match validate_image_url(&request.url) {
                                Ok(url) => match client.get(url).send().await {
                                    Ok(resp) => match resp.bytes().await {
                                        Ok(bytes) => match image::load_from_memory(&bytes) {
                                            Ok(image) => result.image = Some(image),
                                            Err(err) => result.error = Some(err.to_string()),
                                        },
                                        Err(err) => result.error = Some(err.to_string()),
                                    },
                                    Err(err) => result.error = Some(err.to_string()),
                                },
                                Err(err) => result.error = Some(err),
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
    let Ok(image_url) = validate_image_url(image_url) else {
        app.set_details("Cover URL is not a safe HTTP(S) URL.");
        return;
    };
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
    active_fetch: &mut Option<ActiveMetadataFetch>,
) {
    if active_fetch.is_some() {
        return;
    }

    let Some((anime_id, title)) = app.next_metadata_fetch_candidate() else {
        return;
    };
    let fetch_anime_id = anime_id.clone();

    let request = MetadataFetchRequest {
        generation: 0,
        query: title,
        source_id: Some(fetch_anime_id.clone()),
        anime_id: Some(anime_id),
        target: MetadataTarget::List,
        force_refresh: false,
    };

    let abort_handle =
        spawn_metadata_fetch_task(runtime, Arc::clone(resolver), request, result_tx.clone());
    *active_fetch = Some(ActiveMetadataFetch {
        anime_id: Some(fetch_anime_id),
        target: MetadataTarget::List,
        handle: abort_handle,
    });
}

pub(crate) fn drain_metadata_results(
    app: &mut App,
    config: &AppConfig,
    cover_cache: &mut CoverCache,
    image_request_tx: &UnboundedSender<ImageLoadRequest>,
    result_rx: &mut UnboundedReceiver<MetadataFetchResult>,
    active_list_metadata_fetch: &mut Option<ActiveMetadataFetch>,
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
    active_list_metadata_fetch: &mut Option<ActiveMetadataFetch>,
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
    active_list_metadata_fetch: &mut Option<ActiveMetadataFetch>,
) {
    if fetch_result.generation != app.current_manual_metadata_generation() {
        return;
    }
    *active_list_metadata_fetch = None;

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

#[cfg(test)]
mod tests {
    use super::{
        ActiveMetadataFetch, AppConfig, CoverCache, ImageLoadRequest, MetadataFetchResult,
        MetadataTarget, handle_current_refresh_metadata_result, image_client, validate_image_url,
    };
    use crate::app::App;
    use anyhow::anyhow;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::mpsc::unbounded_channel;

    fn unique_temp_path(name: &str) -> String {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should advance")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "animestan-media-{name}-{}-{stamp}.json",
                std::process::id()
            ))
            .display()
            .to_string()
    }

    fn test_config() -> AppConfig {
        AppConfig {
            favorites_path: Some(unique_temp_path("favorites")),
            tracking_path: Some(unique_temp_path("tracking")),
            metadata_cache_path: Some(unique_temp_path("metadata-cache")),
            episodes_cache_path: Some(unique_temp_path("episodes-cache")),
            ..Default::default()
        }
    }

    #[test]
    fn stale_current_refresh_result_keeps_newer_refresh_handle_active() {
        let config = test_config();
        let mut app = App::new();
        let _ = app.next_manual_metadata_generation();
        let mut cover_cache = CoverCache::load(&config);
        let (image_request_tx, _image_request_rx) = unbounded_channel::<ImageLoadRequest>();
        let (abort_handle, _abort_registration) = futures::future::AbortHandle::new_pair();
        let mut active_manual_refresh = Some(ActiveMetadataFetch {
            anime_id: Some("naruto".to_string()),
            target: MetadataTarget::CurrentRefresh,
            handle: abort_handle,
        });
        let stale_result = MetadataFetchResult {
            generation: 0,
            target: MetadataTarget::CurrentRefresh,
            anime_id: Some("naruto".to_string()),
            result: Err(anyhow!("stale refresh")),
        };

        handle_current_refresh_metadata_result(
            &mut app,
            &config,
            &mut cover_cache,
            &image_request_tx,
            stale_result,
            &mut active_manual_refresh,
        );

        assert!(active_manual_refresh.is_some());
    }

    #[test]
    fn image_urls_require_http_host_and_no_credentials() {
        assert!(validate_image_url("https://cdn.example/cover.jpg").is_ok());
        for value in [
            "file:///tmp/cover.jpg",
            "javascript:alert(1)",
            "https://user:password@cdn.example/cover.jpg",
        ] {
            assert!(
                validate_image_url(value).is_err(),
                "unsafe image URL: {value}"
            );
        }
    }

    #[tokio::test]
    async fn image_client_stops_cross_origin_redirects() {
        let origin_listener = TcpListener::bind("127.0.0.1:0").expect("bind origin server");
        let origin_address = origin_listener.local_addr().expect("origin address");
        let external_listener = TcpListener::bind("127.0.0.1:0").expect("bind external server");
        external_listener
            .set_nonblocking(true)
            .expect("set nonblocking");
        let external_address = external_listener.local_addr().expect("external address");

        let server = thread::spawn(move || {
            let (mut stream, _) = origin_listener.accept().expect("accept origin request");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).expect("read origin request");
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{external_address}/cover.jpg\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .expect("write redirect");
        });

        let response = image_client()
            .get(format!("http://{origin_address}/cover.jpg"))
            .send()
            .await
            .expect("request should stop at redirect");

        server.join().expect("origin server thread");
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert!(matches!(
            external_listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }
}
