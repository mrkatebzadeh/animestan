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

use super::{AnimeMetadata, App};

impl App {
    pub fn info_modal_visible(&self) -> bool {
        self.info_modal_visible
    }

    pub fn info_modal_loading(&self) -> bool {
        self.info_modal_loading
    }

    pub fn info_modal_metadata(&self) -> Option<&AnimeMetadata> {
        self.info_modal_metadata.as_ref()
    }

    pub fn info_modal_error(&self) -> Option<&str> {
        self.info_modal_error.as_deref()
    }

    pub fn open_info_modal(&mut self) {
        self.info_modal_visible = true;
        self.info_modal_metadata = None;
        self.info_modal_error = None;
        self.pending_info_fetch = true;
    }

    pub fn close_info_modal(&mut self) {
        self.info_modal_visible = false;
        self.info_modal_loading = false;
        self.pending_info_fetch = false;
    }

    pub fn take_pending_info_fetch(&mut self) -> bool {
        if self.pending_info_fetch {
            self.pending_info_fetch = false;
            true
        } else {
            false
        }
    }

    pub fn next_info_fetch_generation(&mut self) -> u64 {
        self.info_fetch_generation = self.info_fetch_generation.wrapping_add(1);
        self.info_fetch_generation
    }

    pub fn current_info_fetch_generation(&self) -> u64 {
        self.info_fetch_generation
    }

    pub fn set_info_modal_loading(&mut self, loading: bool) {
        self.info_modal_loading = loading;
    }

    pub fn set_info_modal_metadata(&mut self, metadata: AnimeMetadata) {
        self.info_modal_metadata = Some(metadata);
        self.info_modal_error = None;
    }

    pub fn set_info_modal_error(&mut self, error: impl Into<String>) {
        self.info_modal_error = Some(error.into());
        self.info_modal_metadata = None;
    }
}
