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
use std::sync::{Mutex, OnceLock};

use spdlog::{
    Level, LevelFilter, Logger,
    sink::{FileSink, StdStreamSink},
};

use crate::{config::AppConfig, error::Error};

static LOGGER_STATE: OnceLock<()> = OnceLock::new();
static LOGGER_MUTEX: Mutex<()> = Mutex::new(());

/// Initializes global logging sinks for the given `app_name` using an optional stderr sink and a
/// file sink.
///
/// # Errors
///
/// Returns [`Error::LoggingIo`] when the log directory cannot be created, [`Error::LoggingInit`]
/// when sink construction fails, or [`Error::LoggingPoison`] when the initialization mutex is
/// poisoned.
pub fn init_logging(
    app_name: &str,
    verbosity: u8,
    config: &AppConfig,
    console: bool,
) -> Result<(), Error> {
    if LOGGER_STATE.get().is_some() {
        return Ok(());
    }

    let _guard = LOGGER_MUTEX.lock().map_err(|_| Error::LoggingPoison)?;
    if LOGGER_STATE.get().is_some() {
        return Ok(());
    }

    configure_logger(app_name, verbosity, config, console)?;
    let _ = LOGGER_STATE.set(());
    Ok(())
}

fn configure_logger(
    app_name: &str,
    verbosity: u8,
    config: &AppConfig,
    console: bool,
) -> Result<(), Error> {
    let logs_dir = config.logs_dir();
    fs::create_dir_all(&logs_dir).map_err(|source| Error::LoggingIo {
        path: logs_dir.clone(),
        source,
    })?;

    let log_path = logs_dir.join(format!("{app_name}.log"));
    let file_sink = FileSink::builder()
        .path(&log_path)
        .build_arc()
        .map_err(spdlog_error)?;

    let level = level_filter_from(verbosity);
    let mut builder = Logger::builder();
    builder.name(app_name).level_filter(level);

    if console {
        let console_sink = StdStreamSink::builder()
            .stderr()
            .build_arc()
            .map_err(spdlog_error)?;
        builder.sink(console_sink);
    }

    let logger = builder.sink(file_sink).build_arc().map_err(spdlog_error)?;
    logger.set_flush_level_filter(LevelFilter::MoreSevereEqual(Level::Warn));

    spdlog::set_default_logger(logger);
    Ok(())
}

fn level_filter_from(verbosity: u8) -> LevelFilter {
    match verbosity {
        0 => LevelFilter::MoreSevereEqual(Level::Warn),
        1 => LevelFilter::MoreSevereEqual(Level::Info),
        _ => LevelFilter::MoreSevereEqual(Level::Debug),
    }
}

/// Maps spdlog errors into the crate-wide [`Error`] type.
pub(crate) fn spdlog_error(source: spdlog::Error) -> Error {
    Error::LoggingInit { source }
}
