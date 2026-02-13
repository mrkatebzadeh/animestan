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

use std::io;
use std::path::PathBuf;

use spdlog::Error as SpdlogError;
use thiserror::Error;
use url::ParseError;

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
    #[error("could not determine default config path")]
    ConfigPathUnavailable,
    #[error("failed to read config file at '{path}': {source}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse config file at '{path}': {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("unknown source id '{source_id}'")]
    UnknownSourceId { source_id: String },
    #[error("failed to read tracking file at '{path}': {source}")]
    TrackingRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write tracking file at '{path}': {source}")]
    TrackingWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse tracking file at '{path}': {source}")]
    TrackingParse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to read favorites file at '{path}': {source}")]
    FavoritesRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write favorites file at '{path}': {source}")]
    FavoritesWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse favorites file at '{path}': {source}")]
    FavoritesParse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to create downloads directory at '{path}': {source}")]
    DownloadCreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write download file at '{path}': {source}")]
    DownloadWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("download request failed for '{url}': {source}")]
    DownloadRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("unexpected download response for '{url}': {source}")]
    DownloadResponse {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to remove download file at '{path}': {source}")]
    DownloadRemove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse download url: {0}")]
    DownloadUrl(#[source] ParseError),
    #[error("logging initialization mutex poisoned")]
    LoggingPoison,
    #[error("failed to initialize logging: {source}")]
    LoggingInit {
        #[source]
        source: SpdlogError,
    },
    #[error("failed to prepare logs directory at '{path}': {source}")]
    LoggingIo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
