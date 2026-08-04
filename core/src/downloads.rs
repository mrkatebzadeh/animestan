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

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use spdlog::prelude::*;
use url::Url;

use crate::{CoreResult, config::AppConfig, error::Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Downloader {
    YtDlp,
    Ffmpeg,
}

fn downloader_command(downloader: Downloader, stream_url: &Url, target: &Path) -> Command {
    match downloader {
        Downloader::YtDlp => {
            let mut command = Command::new("yt-dlp");
            command.args([
                "--no-part",
                "--no-skip-unavailable-fragments",
                "--fragment-retries",
                "infinite",
                "-N",
                "16",
                "--merge-output-format",
                "mp4",
                "-o",
            ]);
            command.arg(target).arg(stream_url.as_str());
            command
        }
        Downloader::Ffmpeg => {
            let mut command = Command::new("ffmpeg");
            command.args([
                "-y",
                "-extension_picky",
                "0",
                "-loglevel",
                "error",
                "-stats",
                "-i",
            ]);
            command
                .arg(stream_url.as_str())
                .args(["-c", "copy", "-f", "mp4"])
                .arg(target);
            command
        }
    }
}

fn program_available(program: &str) -> bool {
    let suffix = env::consts::EXE_SUFFIX;
    let Some(path) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path)
        .map(|directory| directory.join(format!("{program}{suffix}")))
        .any(|path| program_is_available(&path))
}

fn program_is_available(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn cleanup_temp_file(path: &Path) -> CoreResult<()> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::DownloadWrite {
                path: path.to_path_buf(),
                source,
            }
            .into());
        }
    }
    Ok(())
}

fn prepare_download(
    temp_path: &Path,
    is_available: impl Fn(&str) -> bool,
) -> CoreResult<Downloader> {
    cleanup_temp_file(temp_path)?;
    if is_available("yt-dlp") {
        Ok(Downloader::YtDlp)
    } else if is_available("ffmpeg") {
        Ok(Downloader::Ffmpeg)
    } else {
        Err(Error::DownloadDependency.into())
    }
}

fn finalize_download(temp: &Path, target: &Path, succeeded: bool) -> CoreResult<()> {
    if succeeded {
        let metadata = fs::metadata(temp).map_err(|source| Error::DownloadWrite {
            path: temp.to_path_buf(),
            source,
        })?;
        if !metadata.is_file() {
            return Err(Error::DownloadWrite {
                path: temp.to_path_buf(),
                source: io::Error::new(io::ErrorKind::InvalidData, "download output is not a file"),
            }
            .into());
        }
        if let Err(source) = fs::rename(temp, target) {
            let _ = fs::remove_file(temp);
            return Err(Error::DownloadWrite {
                path: target.to_path_buf(),
                source,
            }
            .into());
        }
    } else {
        cleanup_temp_file(temp)?;
    }

    Ok(())
}

fn validate_download_id(episode_id: &str) -> CoreResult<()> {
    let path = Path::new(episode_id);
    let safe = !episode_id.is_empty()
        && episode_id != "."
        && episode_id != ".."
        && !episode_id.chars().any(char::is_control)
        && !episode_id.chars().any(|character| {
            matches!(character, '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|')
                || (cfg!(windows) && character == ':')
        })
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if safe {
        return Ok(());
    }

    Err(Error::InvalidDownloadId {
        episode_id: episode_id.to_string(),
    }
    .into())
}

/// Returns the final download path for an episode ID.
/// # Errors
///
/// Returns an error when `episode_id` is not a safe single filename component.
pub fn episode_file_path(config: &AppConfig, episode_id: &str) -> CoreResult<PathBuf> {
    validate_download_id(episode_id)?;
    Ok(config.downloads_dir().join(format!("{episode_id}.mp4")))
}

#[must_use]
pub fn local_playback_url(config: &AppConfig, episode_id: &str) -> Option<Url> {
    let path = episode_file_path(config, episode_id).ok()?;
    if !path.exists() {
        return None;
    }

    Url::from_file_path(path).ok()
}

/// Downloads the episode stream to the configured downloads directory.
///
/// # Errors
///
/// Returns an error if the directory cannot be created, no downloader is
/// available, a downloader fails, or the file cannot be written.
pub fn download_episode(
    config: &AppConfig,
    episode_id: &str,
    stream_url: &Url,
) -> CoreResult<PathBuf> {
    crate::client::validate_media_url(stream_url)?;
    info!("starting download for '{episode_id}' from {stream_url}");
    let target_path = episode_file_path(config, episode_id)?;
    let downloads_dir = config.downloads_dir();
    fs::create_dir_all(&downloads_dir).map_err(|source| Error::DownloadCreateDir {
        path: downloads_dir.clone(),
        source,
    })?;

    let temp_path = downloads_dir.join(format!("{episode_id}.mp4.part"));
    if target_path.exists() {
        warn!(
            "local file already exists for '{episode_id}', overwriting {}",
            target_path.display()
        );
    }
    debug!(
        "download paths for '{episode_id}': temp={}, final={}",
        temp_path.display(),
        target_path.display()
    );
    let remote_url = Url::parse(stream_url.as_str()).map_err(Error::DownloadUrl)?;
    let url_display = remote_url.to_string();

    let downloader = prepare_download(&temp_path, program_available)?;
    let program = match downloader {
        Downloader::YtDlp => "yt-dlp",
        Downloader::Ffmpeg => "ffmpeg",
    };

    let mut command = downloader_command(downloader, &remote_url, &temp_path);
    let status = match command.status() {
        Ok(status) => status,
        Err(source) => {
            let _ = finalize_download(&temp_path, &target_path, false);
            return Err(Error::DownloadWrite {
                path: temp_path,
                source,
            }
            .into());
        }
    };
    let succeeded = status.success();
    finalize_download(&temp_path, &target_path, succeeded)?;
    if !succeeded {
        return Err(Error::DownloadProcess {
            program,
            url: url_display,
            status: status.to_string(),
        }
        .into());
    }

    info!(
        "completed download for '{episode_id}' at {}",
        target_path.display()
    );
    Ok(target_path)
}

/// Deletes the downloaded episode file if it exists.
///
/// # Errors
///
/// Returns an error when the filesystem fails to remove the file for reasons
/// other than the file being missing.
pub fn delete_episode(config: &AppConfig, episode_id: &str) -> CoreResult<bool> {
    let path = episode_file_path(config, episode_id)?;
    if !path.exists() {
        return Ok(false);
    }

    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(Error::DownloadRemove { path, source }.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Downloader, cleanup_temp_file, delete_episode, download_episode, downloader_command,
        episode_file_path, finalize_download, local_playback_url, prepare_download,
        program_is_available,
    };
    use crate::config::AppConfig;
    use std::path::Path;
    use url::Url;

    #[test]
    fn builds_yt_dlp_hls_command() {
        let url = Url::parse("https://cdn.example/720/index.m3u8").unwrap();
        let command = downloader_command(Downloader::YtDlp, &url, Path::new("episode.mp4.part"));
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(command.get_program(), "yt-dlp");
        assert!(args.windows(2).any(|pair| pair == ["-N", "16"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--fragment-retries", "infinite"])
        );
        assert!(args.contains(&"--no-part".to_string()));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-o", "episode.mp4.part"])
        );
    }

    #[test]
    fn builds_ffmpeg_stream_copy_command() {
        let url = Url::parse("https://cdn.example/720/index.m3u8").unwrap();
        let command = downloader_command(Downloader::Ffmpeg, &url, Path::new("episode.mp4.part"));
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(command.get_program(), "ffmpeg");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-i", "https://cdn.example/720/index.m3u8"])
        );
        assert!(args.windows(2).any(|pair| pair == ["-c", "copy"]));
        assert!(args.windows(2).any(|pair| pair == ["-f", "mp4"]));
        assert_eq!(args.last(), Some(&"episode.mp4.part".to_string()));
    }

    #[test]
    fn finalization_renames_success_and_removes_failure() {
        let dir = std::env::temp_dir().join(format!("animestan-download-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = dir.join("episode.mp4.part");
        let target = dir.join("episode.mp4");

        std::fs::write(&temp, b"video").unwrap();
        finalize_download(&temp, &target, true).unwrap();
        assert!(target.is_file());

        std::fs::write(&temp, b"partial").unwrap();
        finalize_download(&temp, &target, false).unwrap();
        assert!(!temp.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finalization_removes_temp_when_rename_fails() {
        let dir = std::env::temp_dir().join(format!(
            "animestan-download-rename-failure-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = dir.join("episode.mp4.part");
        let target = dir.join("episode.mp4");

        std::fs::write(&temp, b"video").unwrap();
        std::fs::create_dir(&target).unwrap();
        let error = finalize_download(&temp, &target, true).unwrap_err();

        assert!(!temp.exists());
        assert!(target.is_dir());
        assert!(error.to_string().contains("failed to write download file"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finalization_preserves_output_metadata_error() {
        let dir = std::env::temp_dir().join(format!(
            "animestan-download-metadata-error-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = dir.join("missing.mp4.part");
        let target = dir.join("episode.mp4");

        let error = finalize_download(&temp, &target, true).unwrap_err();
        let crate::Error::DownloadWrite { source, .. } =
            error.downcast_ref::<crate::Error>().unwrap()
        else {
            panic!("expected a download write error");
        };
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        assert!(source.raw_os_error().is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleans_stale_temp_file() {
        let dir = std::env::temp_dir().join(format!(
            "animestan-download-dependency-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = dir.join("episode.mp4.part");
        std::fs::write(&temp, b"partial").unwrap();

        cleanup_temp_file(&temp).unwrap();

        assert!(!temp.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleans_stale_temp_file_before_download_dependency() {
        let dir = std::env::temp_dir().join(format!(
            "animestan-download-dependency-order-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = dir.join("episode.mp4.part");
        std::fs::write(&temp, b"partial").unwrap();

        let error = prepare_download(&temp, |_| false).expect_err("missing downloader");

        assert!(matches!(
            error.downcast_ref::<crate::Error>(),
            Some(crate::Error::DownloadDependency)
        ));
        assert!(!temp.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accepts_safe_legacy_episode_ids_but_rejects_path_inputs() {
        let config = AppConfig::default();

        assert!(episode_file_path(&config, "allanime-show-1").is_ok());
        for episode_id in [
            "",
            ".",
            "..",
            "../escape",
            "12/sidecar",
            r"..\escape",
            "/tmp/escape",
            "episode\0id",
        ] {
            assert!(
                episode_file_path(&config, episode_id).is_err(),
                "unsafe download id: {episode_id:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn legacy_download_ids_are_used_by_local_and_delete_boundaries() {
        let config = AppConfig::default();
        let episode_id = format!("show_id:{}", std::process::id());
        let path = episode_file_path(&config, &episode_id).expect("legacy path");
        std::fs::create_dir_all(path.parent().expect("download parent")).unwrap();
        std::fs::write(&path, b"legacy").unwrap();

        assert!(local_playback_url(&config, &episode_id).is_some());
        assert!(delete_episode(&config, &episode_id).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn download_and_delete_reject_unsafe_episode_ids() {
        let config = AppConfig::default();
        let stream_url = Url::parse("https://cdn.example/master.m3u8").unwrap();

        assert!(download_episode(&config, "../escape", &stream_url).is_err());
        assert!(delete_episode(&config, "../escape").is_err());
    }

    #[test]
    fn rejects_unsafe_download_urls_before_side_effects() {
        let config = AppConfig::default();
        for value in [
            "file:///tmp/master.m3u8",
            "javascript:alert(1)",
            "https://user:password@cdn.example/master.m3u8",
        ] {
            let stream_url = Url::parse(value).unwrap();
            let error = download_episode(&config, "1", &stream_url)
                .expect_err("unsafe download URL should be rejected");

            assert!(
                error.to_string().contains("unsupported media URL"),
                "{value}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn ignores_non_executable_programs() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("animestan-download-program-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("yt-dlp");
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o644);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert!(!program_is_available(&path));

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert!(program_is_available(&path));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
