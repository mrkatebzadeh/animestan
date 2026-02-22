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

use url::form_urlencoded::byte_serialize;

use super::{AnimeMetadata, MetadataSource};

pub(crate) fn decorate_metadata(mut metadata: AnimeMetadata, query: &str) -> AnimeMetadata {
    metadata.source = MetadataSource::AllManga;
    metadata.source_url = source_url(&metadata.title, query);
    metadata
}

fn source_url(title: &str, query: &str) -> String {
    let search = if title.trim().is_empty() {
        query
    } else {
        title
    };
    let encoded: String = byte_serialize(search.as_bytes()).collect();
    format!("https://allmanga.to/anime?search={encoded}")
}
