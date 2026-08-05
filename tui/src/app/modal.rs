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
        self.modal.info_visible
    }

    pub fn info_modal_loading(&self) -> bool {
        self.modal.info_loading
    }

    pub fn info_modal_metadata(&self) -> Option<&AnimeMetadata> {
        self.modal.info_metadata.as_ref()
    }

    pub fn info_modal_error(&self) -> Option<&str> {
        self.modal.info_error.as_deref()
    }

    pub fn open_info_modal(&mut self) {
        self.modal.info_visible = true;
        self.modal.info_metadata = None;
        self.modal.info_error = None;
        self.request_info_metadata();
    }

    pub fn close_info_modal(&mut self) {
        self.modal.info_visible = false;
        self.modal.info_loading = false;
        self.modal.pending_info_fetch = false;
    }

    pub fn take_pending_info_fetch(&mut self) -> bool {
        std::mem::take(&mut self.modal.pending_info_fetch)
    }

    pub fn next_info_fetch_generation(&mut self) -> u64 {
        self.modal.info_fetch_generation = self.modal.info_fetch_generation.wrapping_add(1);
        self.modal.info_fetch_generation
    }

    pub fn current_info_fetch_generation(&self) -> u64 {
        self.modal.info_fetch_generation
    }

    pub fn set_info_modal_loading(&mut self, loading: bool) {
        self.modal.info_loading = loading;
    }

    pub fn set_info_modal_metadata(&mut self, metadata: AnimeMetadata) {
        self.modal.info_metadata = Some(metadata);
        self.modal.info_error = None;
    }

    pub fn set_info_modal_error(&mut self, error: impl Into<String>) {
        self.modal.info_error = Some(error.into());
        self.modal.info_metadata = None;
    }

    pub fn request_info_metadata(&mut self) {
        self.modal.pending_info_fetch = true;
        self.modal.info_metadata = None;
        self.modal.info_error = None;
        self.modal.info_loading = true;
    }
}
