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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories_next::ProjectDirs;
use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub use_fixtures: Option<bool>,
    #[serde(default)]
    pub player: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
}

impl AppConfig {
    #[must_use]
    pub fn default_path() -> PathBuf {
        Self::resolve_default_path().unwrap_or_else(|_| PathBuf::from("animestan/config.toml"))
    }

    /// Loads configuration from the default config path, falling back to an empty
    /// [`AppConfig`] when the file is absent.
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory cannot be determined, the file
    /// cannot be read from disk, or its contents cannot be parsed.
    pub fn load_default() -> Result<Self, Error> {
        let path = Self::resolve_default_path()?;
        match fs::read_to_string(&path) {
            Ok(contents) => Self::parse(&contents, path),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(Error::ConfigRead { path, source }),
        }
    }

    /// Loads configuration from the provided `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read from disk or if the
    /// contents fail to parse.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path).map_err(|source| Error::ConfigRead {
            path: path.clone(),
            source,
        })?;
        Self::parse(&contents, path)
    }

    fn resolve_default_path() -> Result<PathBuf, Error> {
        if let Some(project_dirs) = ProjectDirs::from("xyz", "Animestan", "animestan") {
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

        Err(Error::ConfigPathUnavailable)
    }

    fn parse(contents: &str, path: PathBuf) -> Result<Self, Error> {
        toml::from_str(contents).map_err(|source| Error::ConfigParse { path, source })
    }
}
