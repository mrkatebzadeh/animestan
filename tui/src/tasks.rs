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
use std::time::Duration;

use animestan_core::{
    AnimeClient, AnimeMetadata, AppConfig, CoreResult, Episode, EpisodeTracker, FetchBackend,
    MetadataProvider, MetadataResolver, local_playback_url,
};
use anyhow::{Result, anyhow};
use futures::future::{AbortHandle, Abortable};
use spdlog::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::sleep;

use crate::cache::{EpisodeCache, cache_episodes};
use crate::playback;

pub(crate) struct EpisodeFetchRequest {
    pub(crate) generation: u64,
    pub(crate) anime_id: String,
}

pub(crate) struct EpisodeFetchResult {
    pub(crate) generation: u64,
    pub(crate) anime_id: String,
    pub(crate) result: CoreResult<Vec<Episode>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MetadataTarget {
    InfoModal,
    SearchResults,
    List,
    CurrentRefresh,
    Background,
}

pub(crate) struct MetadataFetchRequest {
    pub(crate) generation: u64,
    pub(crate) query: String,
    pub(crate) source_id: Option<String>,
    pub(crate) anime_id: Option<String>,
    pub(crate) target: MetadataTarget,
    pub(crate) force_refresh: bool,
}

pub(crate) struct MetadataFetchResult {
    pub(crate) generation: u64,
    pub(crate) target: MetadataTarget,
    pub(crate) anime_id: Option<String>,
    pub(crate) result: Result<AnimeMetadata>,
}

#[derive(Clone)]
pub(crate) struct PlaybackRequest {
    pub(crate) episode_id: String,
    pub(crate) episode_title: Option<String>,
}

pub(crate) struct PlaybackResult {
    pub(crate) episode_title: Option<String>,
    pub(crate) outcome: Result<()>,
}

pub(crate) fn spawn_episode_fetch_task(
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

pub(crate) fn spawn_metadata_fetch_task(
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
pub(crate) fn spawn_background_episode_refresh_tasks(
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
                        match blocking_result {
                            Ok(Ok(episodes)) => {
                                if let Err(err) = cache_episodes(&cache, &anime_id, &episodes) {
                                    warn!(
                                        "failed to cache background episodes for '{}': {}",
                                        anime_id, err
                                    );
                                }
                            }
                            Ok(Err(err)) => {
                                warn!(
                                    "background episode refresh failed for '{}': {}",
                                    anime_id, err
                                );
                            }
                            Err(err) => {
                                warn!(
                                    "background episode refresh join failed for '{}': {}",
                                    anime_id, err
                                );
                            }
                        }
                        let _ = job_tx.send(());
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

pub(crate) fn spawn_background_metadata_refresh_tasks(
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

pub(crate) fn spawn_playback_task(
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

fn mark_episode_started(tracker: &Arc<Mutex<EpisodeTracker>>, episode_id: &str) -> Result<()> {
    let mut guard = tracker
        .lock()
        .map_err(|_| anyhow!("episode tracker lock poisoned"))?;
    guard.mark_started(episode_id)?;
    Ok(())
}
