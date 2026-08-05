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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories_next::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};

use crate::{CoreResult, error::Error};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub metadata_cache_path: Option<String>,
    #[serde(default)]
    pub episodes_cache_path: Option<String>,
    #[serde(default)]
    pub use_fixtures: Option<bool>,
    #[serde(default)]
    pub player: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub tracking_path: Option<String>,
    #[serde(default)]
    pub favorites_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamingMode {
    Sub,
    Dub,
}

impl StreamingMode {
    pub(crate) fn parse(value: Option<&str>) -> CoreResult<Self> {
        match value.unwrap_or("sub") {
            "sub" => Ok(Self::Sub),
            "dub" => Ok(Self::Dub),
            value => Err(Error::InvalidConfigValue {
                key: "mode",
                value: value.to_string(),
                expected: "sub or dub",
            }
            .into()),
        }
    }

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Sub => "jpn",
            Self::Dub => "eng",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QualityPreference {
    Best,
    Worst,
    Height(u16),
}

impl QualityPreference {
    pub(crate) fn parse(value: Option<&str>) -> CoreResult<Self> {
        let value = value.unwrap_or("best");
        let invalid = || Error::InvalidConfigValue {
            key: "quality",
            value: value.to_string(),
            expected: "best, worst, or a positive height followed by p",
        };

        match value {
            "best" => Ok(Self::Best),
            "worst" => Ok(Self::Worst),
            _ => {
                let Some(height) = value.strip_suffix('p') else {
                    return Err(invalid().into());
                };
                if height.is_empty() || !height.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(invalid().into());
                }
                let height = height.parse::<u16>().map_err(|_| invalid())?;
                if height == 0 {
                    return Err(invalid().into());
                }
                Ok(Self::Height(height))
            }
        }
    }
}

impl AppConfig {
    #[must_use]
    pub fn default_path() -> PathBuf {
        Self::resolve_default_path().unwrap_or_else(|_| PathBuf::from("animestan/config.toml"))
    }

    /// Returns the default configuration as a TOML string.
    #[must_use]
    pub fn default_toml() -> String {
        r#"# Animestan configuration file
# Uncomment and modify any settings below as needed

# Built-in anime and metadata source: AniDB

# Audio mode: sub or dub (default: sub)
# mode = "sub"

# Streaming quality: best, worst, or a height such as 720p (default: best)
# quality = "best"

# HLS downloads require yt-dlp or ffmpeg; yt-dlp is preferred.
# New state lives under the anidb namespace; old source-dependent state is preserved but ignored.
# Clear or change custom paths when upgrading; downloads remain available.

# Path to cached metadata (relative to config dir or absolute)
# metadata_cache_path = "metadata_cache.json"

# Path to cached episode lists (relative to config dir or absolute)
# episodes_cache_path = "episodes_cache.json"

# Media player command (default: mpv)
# player = "mpv"

# Path to episode tracking file (relative to config dir or absolute)
# tracking_path = "progress.json"

# Path to favorites file (relative to config dir or absolute)
# favorites_path = "favorites.json"
"#
        .to_string()
    }

    /// Loads configuration from the default config path, falling back to an empty
    /// [`AppConfig`] when the file is absent.
    ///
    /// If the config file does not exist, it will be created with default settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory cannot be determined, the file
    /// cannot be read from disk, or its contents cannot be parsed.
    pub fn load_default() -> CoreResult<Self> {
        let path = Self::resolve_default_path()?;
        match fs::read_to_string(&path) {
            Ok(contents) => Self::parse(&contents, path),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // Create the config directory if it doesn't exist
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|source| Error::ConfigWrite {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }
                // Write default config to file
                let default_toml = Self::default_toml();
                fs::write(&path, &default_toml).map_err(|source| Error::ConfigWrite {
                    path: path.clone(),
                    source,
                })?;
                Ok(Self::default())
            }
            Err(source) => Err(Error::ConfigRead { path, source }.into()),
        }
    }

    #[must_use]
    pub fn config_dir() -> PathBuf {
        Self::default_path()
            .parent()
            .map_or_else(|| PathBuf::from("animestan"), Path::to_path_buf)
    }

    #[must_use]
    pub fn data_dir() -> PathBuf {
        ProjectDirs::from("", "", "animestan")
            .map_or_else(Self::config_dir, |dirs| dirs.data_dir().to_path_buf())
    }

    #[must_use]
    pub fn progress_path(&self) -> PathBuf {
        Self::resolve_path(self.tracking_path.as_deref(), "progress.json")
    }

    #[must_use]
    pub fn favorites_path(&self) -> PathBuf {
        Self::resolve_path(self.favorites_path.as_deref(), "favorites.json")
    }

    #[must_use]
    pub fn metadata_cache_path(&self) -> PathBuf {
        Self::resolve_path(self.metadata_cache_path.as_deref(), "metadata_cache.json")
    }

    #[must_use]
    pub fn episodes_cache_path(&self) -> PathBuf {
        Self::resolve_path(self.episodes_cache_path.as_deref(), "episodes_cache.json")
    }

    #[must_use]
    pub fn downloads_dir(&self) -> PathBuf {
        let _ = self;

        if cfg!(target_os = "linux") {
            if let Some(base_dirs) = BaseDirs::new() {
                let mut path = base_dirs.home_dir().to_path_buf();
                path.push(".local");
                path.push("share");
                path.push("animestan");
                path.push("downloads");
                return path;
            }
        }

        Self::data_dir().join("downloads")
    }

    #[must_use]
    pub fn logs_dir(&self) -> PathBuf {
        let _ = self;

        if cfg!(target_os = "linux") {
            if let Some(base_dirs) = BaseDirs::new() {
                let mut path = base_dirs.home_dir().to_path_buf();
                path.push(".local");
                path.push("share");
                path.push("animestan");
                path.push("logs");
                return path;
            }
        }

        Self::data_dir().join("logs")
    }

    #[must_use]
    pub fn covers_dir(&self) -> PathBuf {
        let _ = self;

        Self::data_dir().join("anidb").join("covers")
    }

    /// Loads configuration from the provided `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read from disk or if the
    /// contents fail to parse.
    pub fn load_from(path: impl AsRef<Path>) -> CoreResult<Self> {
        let path = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path).map_err(|source| Error::ConfigRead {
            path: path.clone(),
            source,
        })?;
        Self::parse(&contents, path)
    }

    fn resolve_default_path() -> CoreResult<PathBuf> {
        if let Some(project_dirs) = ProjectDirs::from("", "", "animestan") {
            return Ok(project_dirs.config_dir().join("config.toml"));
        }

        #[cfg(unix)]
        {
            if let Some(home) = std::env::var_os("HOME") {
                let path = PathBuf::from(home)
                    .join(".config")
                    .join("animestan")
                    .join("config.toml");
                return Ok(path);
            }
        }

        Err(Error::ConfigPathUnavailable.into())
    }

    fn resolve_path(configured: Option<&str>, default_filename: &str) -> PathBuf {
        let config_dir = Self::config_dir();
        configured.map_or_else(
            || config_dir.join("anidb").join(default_filename),
            |path| {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    path
                } else {
                    config_dir.join(path)
                }
            },
        )
    }

    fn parse(contents: &str, path: PathBuf) -> CoreResult<Self> {
        let config =
            toml::from_str(contents).map_err(|source| Error::ConfigParse { path, source })?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{AppConfig, QualityPreference, StreamingMode};

    #[test]
    fn defaults_namespace_anidb_state() {
        let config = AppConfig::default();
        assert!(config.favorites_path().ends_with("anidb/favorites.json"));
        assert!(config.progress_path().ends_with("anidb/progress.json"));
        assert!(
            config
                .metadata_cache_path()
                .ends_with("anidb/metadata_cache.json")
        );
        assert!(
            config
                .episodes_cache_path()
                .ends_with("anidb/episodes_cache.json")
        );
        assert!(config.covers_dir().ends_with("anidb/covers"));
    }

    #[test]
    fn default_path_joins_anidb_namespace_and_filename() {
        let expected = AppConfig::config_dir().join("anidb").join("progress.json");

        assert_eq!(AppConfig::resolve_path(None, "progress.json"), expected);
    }

    #[test]
    fn config_parses_streaming_mode() {
        let config = AppConfig::parse(
            "mode = \"dub\"\nquality = \"720p\"",
            PathBuf::from("test.toml"),
        )
        .expect("test configuration should parse");
        assert_eq!(config.mode.as_deref(), Some("dub"));
        assert_eq!(config.quality.as_deref(), Some("720p"));
    }

    #[test]
    fn parses_configured_mode_and_quality() {
        assert_eq!(StreamingMode::parse(Some("dub")).unwrap().code(), "eng");
        assert_eq!(
            QualityPreference::parse(Some("720p")).unwrap(),
            QualityPreference::Height(720)
        );
        assert!(StreamingMode::parse(Some("french")).is_err());
        assert!(QualityPreference::parse(Some("hd")).is_err());
    }

    #[test]
    fn rejects_invalid_quality_boundaries() {
        assert!(QualityPreference::parse(Some("0p")).is_err());
        assert!(QualityPreference::parse(Some("65536p")).is_err());
        assert!(QualityPreference::parse(Some("7 20p")).is_err());
        assert_eq!(
            QualityPreference::parse(None).unwrap(),
            QualityPreference::Best
        );
    }

    #[test]
    fn rejects_non_literal_mode_and_quality_values() {
        assert!(StreamingMode::parse(Some(" Dub")).is_err());
        assert!(StreamingMode::parse(Some("DUB")).is_err());
        assert!(QualityPreference::parse(Some(" 720p")).is_err());
        assert!(QualityPreference::parse(Some("720p ")).is_err());
        assert!(QualityPreference::parse(Some("720P")).is_err());
    }

    #[test]
    fn explicit_path_overrides_remain_unchanged() {
        let config = AppConfig {
            metadata_cache_path: Some("cache/metadata.json".to_string()),
            episodes_cache_path: Some("/tmp/episodes.json".to_string()),
            tracking_path: Some("progress.json".to_string()),
            favorites_path: Some("/tmp/favorites.json".to_string()),
            ..AppConfig::default()
        };

        assert_eq!(
            config.metadata_cache_path(),
            AppConfig::config_dir().join("cache/metadata.json")
        );
        assert_eq!(
            config.episodes_cache_path(),
            PathBuf::from("/tmp/episodes.json")
        );
        assert!(config.progress_path().ends_with("progress.json"));
        assert_eq!(
            config.favorites_path(),
            PathBuf::from("/tmp/favorites.json")
        );
    }
}
