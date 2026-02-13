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

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use reqwest::blocking::Client;
use url::Url;

use crate::config::AppConfig;
use crate::error::Error;

const DOWNLOAD_TIMEOUT_SECS: u64 = 60;

#[must_use]
pub fn episode_file_path(config: &AppConfig, episode_id: &str) -> PathBuf {
    config.downloads_dir().join(format!("{episode_id}.mp4"))
}

#[must_use]
pub fn local_playback_url(config: &AppConfig, episode_id: &str) -> Option<Url> {
    let path = episode_file_path(config, episode_id);
    if !path.exists() {
        return None;
    }

    Url::from_file_path(path).ok()
}

/// Downloads the episode stream to the configured downloads directory.
///
/// # Errors
///
/// Returns an error if the directory cannot be created, the request fails,
/// the response status is not successful, or the file cannot be written.
pub fn download_episode(
    config: &AppConfig,
    episode_id: &str,
    stream_url: &Url,
) -> Result<PathBuf, Error> {
    let downloads_dir = config.downloads_dir();
    fs::create_dir_all(&downloads_dir).map_err(|source| Error::DownloadCreateDir {
        path: downloads_dir.clone(),
        source,
    })?;

    let target_path = episode_file_path(config, episode_id);
    let temp_path = downloads_dir.join(format!("{episode_id}.mp4.part"));
    let remote_url = Url::parse(stream_url.as_str()).map_err(Error::DownloadUrl)?;
    let url_display = remote_url.to_string();

    let client = Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|source| Error::DownloadRequest {
            url: url_display.clone(),
            source,
        })?;

    let mut response = client
        .get(remote_url.clone())
        .send()
        .map_err(|source| Error::DownloadRequest {
            url: url_display.clone(),
            source,
        })?
        .error_for_status()
        .map_err(|source| Error::DownloadResponse {
            url: url_display,
            source,
        })?;

    let mut file = File::create(&temp_path).map_err(|source| Error::DownloadWrite {
        path: temp_path.clone(),
        source,
    })?;

    io::copy(&mut response, &mut file).map_err(|source| Error::DownloadWrite {
        path: temp_path.clone(),
        source,
    })?;

    file.flush().map_err(|source| Error::DownloadWrite {
        path: temp_path.clone(),
        source,
    })?;

    fs::rename(&temp_path, &target_path).map_err(|source| Error::DownloadWrite {
        path: target_path.clone(),
        source,
    })?;

    Ok(target_path)
}

/// Deletes the downloaded episode file if it exists.
///
/// # Errors
///
/// Returns an error when the filesystem fails to remove the file for reasons
/// other than the file being missing.
pub fn delete_episode(config: &AppConfig, episode_id: &str) -> Result<bool, Error> {
    let path = episode_file_path(config, episode_id);
    if !path.exists() {
        return Ok(false);
    }

    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(Error::DownloadRemove { path, source }),
    }
}
