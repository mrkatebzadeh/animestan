// Copyright (C) 2026 M.R. Siavash Katebzadeg <mr@katebzadeh.xyz>
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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to load catalog fixture: {0}")]
    CatalogFixture(#[source] serde_json::Error),
    #[error("failed to load response fixture: {0}")]
    ResponseFixture(#[source] serde_json::Error),
    #[error("no sources available in fixture catalog")]
    EmptyCatalog,
    #[error("missing fixture response for url '{url}'")]
    MissingFixture { url: String },
    #[error("failed to render url from template '{template}': {source}")]
    InvalidUrl {
        template: String,
        #[source]
        source: url::ParseError,
    },
    #[error("failed to construct http client: {0}")]
    HttpClient(#[source] reqwest::Error),
    #[error("http request to '{url}' failed: {source}")]
    HttpRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("unexpected http status {status} for '{url}'")]
    HttpStatus { url: String, status: u16 },
    #[error("failed to decode http body for '{url}': {source}")]
    HttpBodyParse {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to parse response for '{url}': {source}")]
    ResponseParse {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to parse stream url '{url}': {source}")]
    StreamUrlParse {
        url: String,
        #[source]
        source: url::ParseError,
    },
    #[error("failed to split episode id '{episode_id}'")]
    EpisodeIdParse { episode_id: String },
    #[error("unable to resolve stream: {message}")]
    StreamResolution { message: String },
}
