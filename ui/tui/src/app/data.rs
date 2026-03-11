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

use super::{
    AnimeMetadata, AnimeProgress, App, Episode, EpisodeIndicators, FavoriteEntry, FilterActive,
    FilterTarget, MetadataSummary, SearchModal,
};
use std::collections::HashMap;

use animestan_core::{AnimeClient, CoreResult, FavoriteStore, FetchBackend};

impl App {
    pub fn bookmark_entries(&self) -> &[FavoriteEntry] {
        self.visible_bookmark_entries()
    }

    pub fn episodes(&self) -> &[Episode] {
        self.visible_episodes()
    }

    pub fn set_episodes_loading(&mut self, loading: bool) {
        self.data.episodes_loading = loading;
    }

    pub fn episodes_loading(&self) -> bool {
        self.data.episodes_loading
    }

    pub fn next_fetch_generation(&mut self) -> u64 {
        self.data.fetch_generation = self.data.fetch_generation.wrapping_add(1);
        self.data.fetch_generation
    }

    pub fn current_fetch_generation(&self) -> u64 {
        self.data.fetch_generation
    }

    pub fn unfiltered_episodes(&self) -> &[Episode] {
        &self.data.episodes
    }

    pub fn set_episode_indicators(&mut self, indicators: HashMap<String, EpisodeIndicators>) {
        self.data.episode_indicators = indicators;
    }

    pub fn episode_indicators(&self, episode_id: &str) -> EpisodeIndicators {
        self.data
            .episode_indicators
            .get(episode_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn load_bookmarks(&mut self, store: &FavoriteStore) {
        self.data.bookmark_entries = store.list();
        self.apply_saved_panel_filter(FilterTarget::Bookmarks);
        self.reset_navigation_state();
        if self.data.bookmark_entries.is_empty() {
            self.set_details("No bookmarks saved yet. Use the CLI to add some.");
            self.nav.anime_selection_changed = false;
        } else {
            self.set_details(format!(
                "Loaded {} bookmarks",
                self.data.bookmark_entries.len()
            ));
            self.nav.anime_selection_changed = true;
        }
        self.refresh_quick_launch_items();
    }

    pub fn sync_bookmark_cache(&mut self, store: &FavoriteStore) {
        self.data.bookmark_entries = store.list();
        self.apply_saved_panel_filter(FilterTarget::Bookmarks);
        self.nav.anime_selection_changed = !self.data.bookmark_entries.is_empty();
        self.refresh_quick_launch_items();
    }

    pub fn set_episodes(&mut self, episodes: Vec<Episode>) {
        self.data.episodes = episodes;
        self.data.filtered_episodes.clear();
        self.data.filtered_episode_entries.clear();
        self.data.episode_indicators.clear();
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.nav.right_index = 0;
        self.nav.selected_episode = None;
        self.data.episodes_loading = false;
    }

    pub fn anime_progress_for(&self, anime_id: &str) -> Option<AnimeProgress> {
        self.data.anime_progress.get(anime_id).copied()
    }

    pub fn set_anime_progress(&mut self, anime_id: String, progress: AnimeProgress) {
        self.data.anime_progress.insert(anime_id, progress);
    }

    pub fn metadata_summary(&self, anime_id: &str) -> Option<MetadataSummary> {
        self.data
            .metadata_store
            .get(anime_id)
            .map(|metadata| MetadataSummary {
                status: metadata.status.clone(),
                score: metadata.score,
            })
    }

    pub fn episode_refresh_pending(&self, anime_id: &str) -> bool {
        self.data.episode_refresh_pending.contains(anime_id)
    }

    pub fn mark_episode_refresh_pending(&mut self, anime_id: String) {
        self.data.episode_refresh_pending.insert(anime_id);
    }

    pub fn clear_episode_refresh_pending(&mut self, anime_id: &str) {
        self.data.episode_refresh_pending.remove(anime_id);
    }

    pub fn cached_metadata(&self, anime_id: &str) -> Option<&AnimeMetadata> {
        self.data.metadata_store.get(anime_id)
    }

    pub fn next_metadata_fetch_candidate(&mut self) -> Option<(String, String)> {
        let candidates: Vec<(String, String)> = self
            .visible_bookmark_entries()
            .iter()
            .map(|entry| (entry.anime.id.clone(), entry.anime.title.clone()))
            .collect();

        for (anime_id, title) in candidates {
            if self.should_fetch_metadata(&anime_id) {
                self.data.metadata_pending.insert(anime_id.clone());
                return Some((anime_id, title));
            }
        }
        None
    }

    pub fn store_metadata(&mut self, anime_id: &str, metadata: &AnimeMetadata) {
        let merged = if let Some(existing) = self.data.metadata_store.get(anime_id) {
            merge_metadata(existing, metadata)
        } else {
            metadata.clone()
        };
        self.data
            .metadata_store
            .insert(anime_id.to_string(), merged);
        self.data.metadata_pending.remove(anime_id);
        self.data.metadata_failed.remove(anime_id);
    }

    pub fn cached_metadata_for_current_anime(&self) -> Option<&AnimeMetadata> {
        self.current_anime_id()
            .and_then(|anime_id| self.cached_metadata(&anime_id))
    }

    pub fn set_metadata_failure(&mut self, anime_id: &str) {
        self.data.metadata_pending.remove(anime_id);
        self.data.metadata_failed.insert(anime_id.to_string());
    }

    pub fn record_selected_anime_progress(&mut self) {
        let Some(anime) = self.current_anime() else {
            return;
        };

        if let Some(progress) = self.compute_anime_progress() {
            self.data.anime_progress.insert(anime.id.clone(), progress);
        }
    }

    fn compute_anime_progress(&self) -> Option<AnimeProgress> {
        let episodes = self.unfiltered_episodes();
        if episodes.is_empty() {
            return None;
        }

        let watched = episodes
            .iter()
            .filter(|episode| self.episode_indicators(&episode.id).watched)
            .count();

        Some(AnimeProgress {
            watched,
            total: episodes.len(),
        })
    }

    pub fn clear_episodes(&mut self) {
        self.data.episodes.clear();
        self.data.filtered_episodes.clear();
        self.data.filtered_episode_entries.clear();
        self.data.episode_indicators.clear();
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.nav.right_index = 0;
        self.nav.selected_episode = None;
        self.data.episodes_loading = false;
    }

    pub fn set_filtered_episodes(&mut self, episodes: Vec<Episode>) {
        self.data.filtered_episodes = episodes;
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.nav.right_index = 0;
        self.nav.selected_episode = None;
    }

    pub fn clear_filtered_episodes(&mut self) {
        self.data.filtered_episodes.clear();
        self.apply_saved_panel_filter(FilterTarget::Episodes);
        self.nav.right_index = 0;
        self.nav.selected_episode = None;
    }

    pub fn is_bookmarked(&self, anime_id: &str) -> bool {
        self.data
            .bookmark_entries
            .iter()
            .any(|entry| entry.anime.id == anime_id)
    }

    pub fn toggle_bookmark(&mut self, store: &mut FavoriteStore) -> CoreResult<()> {
        let Some(anime) = self.current_anime().cloned() else {
            self.set_details("Highlight an anime to toggle bookmarks.");
            return Ok(());
        };

        let anime_id = anime.id.clone();
        let details = if self.is_bookmarked(&anime_id) {
            if store.remove(&anime_id)? {
                format!("Removed {} from bookmarks", anime.title)
            } else {
                format!("{} was not bookmarked", anime.title)
            }
        } else {
            store.add(anime.clone())?;
            format!("Added {} to bookmarks", anime.title)
        };

        self.sync_bookmark_cache(store);
        self.set_details(details);
        if matches!(self.search.modal_visible, SearchModal::Visible) {
            self.close_search_results_modal();
        }
        Ok(())
    }

    pub fn add_current_search_result_to_bookmarks(
        &mut self,
        store: &mut FavoriteStore,
    ) -> CoreResult<()> {
        let Some(anime) = self.current_search_result().cloned() else {
            self.set_details("Highlight an anime to add to the anime panel.");
            return Ok(());
        };

        if self.is_bookmarked(&anime.id) {
            self.set_details(format!("{} is already in the anime panel.", anime.title));
            self.close_search_results_modal();
            return Ok(());
        }

        store.add(anime.clone())?;
        self.sync_bookmark_cache(store);
        self.set_details(format!("Added {} to the anime panel.", anime.title));
        self.close_search_results_modal();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn load_episodes(&mut self, client: &AnimeClient<FetchBackend>) -> CoreResult<()> {
        let Some(anime) = self.current_anime() else {
            self.clear_episodes();
            self.set_details("Select an anime to load episodes.");
            return Ok(());
        };

        let episodes = client.list_episodes(&anime.id)?;
        self.set_episodes(episodes);
        self.set_details(format!("Loaded {} episodes", self.data.episodes.len()));
        self.nav.anime_selection_changed = false;
        Ok(())
    }

    pub(super) fn visible_episodes(&self) -> &[Episode] {
        if matches!(self.filters.episode_active, FilterActive::Active) {
            &self.data.filtered_episode_entries
        } else {
            self.base_episode_entries()
        }
    }

    pub(super) fn base_episode_entries(&self) -> &[Episode] {
        if self.current_filter().is_some() {
            &self.data.filtered_episodes
        } else {
            &self.data.episodes
        }
    }

    pub(super) fn visible_bookmark_entries(&self) -> &[FavoriteEntry] {
        if matches!(self.filters.bookmark_active, FilterActive::Active) {
            &self.data.filtered_bookmark_entries
        } else {
            &self.data.bookmark_entries
        }
    }

    fn should_fetch_metadata(&self, anime_id: &str) -> bool {
        !self.data.metadata_store.contains_key(anime_id)
            && !self.data.metadata_pending.contains(anime_id)
            && !self.data.metadata_failed.contains(anime_id)
    }
}

fn merge_metadata(existing: &AnimeMetadata, incoming: &AnimeMetadata) -> AnimeMetadata {
    let synopsis = incoming
        .synopsis
        .as_ref()
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .or_else(|| existing.synopsis.clone());
    let score = incoming.score.or(existing.score);
    let status = incoming.status.clone().or_else(|| existing.status.clone());
    let season = incoming.season.clone().or_else(|| existing.season.clone());
    let year = incoming.year.or(existing.year);
    let trailer_url = incoming
        .trailer_url
        .clone()
        .or_else(|| existing.trailer_url.clone());
    let genres = if incoming.genres.is_empty() {
        existing.genres.clone()
    } else {
        incoming.genres.clone()
    };
    let studios = if incoming.studios.is_empty() {
        existing.studios.clone()
    } else {
        incoming.studios.clone()
    };
    let title = if incoming.title.trim().is_empty() {
        existing.title.clone()
    } else {
        incoming.title.clone()
    };
    let image_url = incoming
        .image_url
        .clone()
        .or_else(|| existing.image_url.clone());
    let source_url = if incoming.source_url.trim().is_empty() {
        existing.source_url.clone()
    } else {
        incoming.source_url.clone()
    };
    AnimeMetadata {
        title,
        synopsis,
        score,
        genres,
        studios,
        status,
        season,
        year,
        trailer_url,
        image_url,
        source_url,
        source: incoming.source,
    }
}
