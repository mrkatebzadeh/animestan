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

use animestan_core::{AppConfig, EpisodeTracker};

use std::{
    ffi::OsStr,
    io,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

#[cfg(unix)]
use serde_json::{Value, json};
#[cfg(unix)]
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    thread,
    time::Duration,
};

#[cfg_attr(not(unix), allow(dead_code))]
const WATCHED_THRESHOLD: f64 = 0.92;
#[cfg(unix)]
const MAX_ATTEMPTS: usize = 20;
#[cfg(unix)]
const RETRY_DELAY_MS: u64 = 100;

pub fn play_episode(
    config: &AppConfig,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    stream_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    play_episode_inner(config, tracker, episode_id, stream_url)
}

#[cfg(unix)]
fn play_episode_inner(
    config: &AppConfig,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    stream_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (binary, extra_args) = player_command(config);
    let mut command = Command::new(&binary);
    command.args(&extra_args);

    if is_mpv(&binary) && needs_allanime_referer(stream_url) {
        command.arg("--referrer=https://allmanga.to");
    }

    command
        .arg(stream_url)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut ipc_socket_path = None;
    if is_mpv(&binary) {
        let path = socket_path(std::process::id());
        let _ = std::fs::remove_file(&path);
        command.arg(format!("--input-ipc-server={}", path.display()));
        ipc_socket_path = Some(path);
    }

    let mut child = command.spawn()?;

    let Some(ipc_socket_path) = ipc_socket_path else {
        finalize_without_ipc(tracker, episode_id, &mut child)?;
        return Ok(());
    };

    let Some(stream) = connect_with_retry(&ipc_socket_path, MAX_ATTEMPTS, RETRY_DELAY_MS) else {
        finalize_without_ipc(tracker, episode_id, &mut child)?;
        let _ = std::fs::remove_file(&ipc_socket_path);
        return Ok(());
    };

    let Ok(mut writer) = stream.try_clone() else {
        finalize_without_ipc(tracker, episode_id, &mut child)?;
        let _ = std::fs::remove_file(&ipc_socket_path);
        return Ok(());
    };

    for (id, property) in [(1, "time-pos"), (2, "duration"), (3, "eof-reached")] {
        if observe_property(&mut writer, id, property).is_err() {
            finalize_without_ipc(tracker, episode_id, &mut child)?;
            let _ = std::fs::remove_file(&ipc_socket_path);
            return Ok(());
        }
    }

    let tracker_for_thread = Arc::clone(tracker);
    let episode_key = episode_id.to_string();
    let handle = thread::spawn(move || ipc_loop(stream, &tracker_for_thread, &episode_key));

    let _ = child.wait();
    let _ = handle.join();
    let _ = std::fs::remove_file(&ipc_socket_path);

    Ok(())
}

#[cfg(not(unix))]
fn play_episode_inner(
    config: &AppConfig,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    stream_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (binary, extra_args) = player_command(config);
    let mut command = Command::new(&binary);
    command.args(&extra_args);

    if is_mpv(&binary) && needs_allanime_referer(stream_url) {
        command.arg("--referrer=https://allmanga.to");
    }

    command
        .arg(stream_url)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = command.spawn()?;
    finalize_without_ipc(tracker, episode_id, &mut child)
}

fn player_command(config: &AppConfig) -> (String, Vec<String>) {
    let raw = config
        .player
        .as_deref()
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .unwrap_or("mpv");

    let mut parts: Vec<String> = raw.split_whitespace().map(ToString::to_string).collect();
    if parts.is_empty() {
        parts.push("mpv".to_string());
    }

    let binary = parts.remove(0);
    (binary, parts)
}

fn is_mpv(binary: &str) -> bool {
    PathBuf::from(binary)
        .file_name()
        .is_some_and(|name| name == OsStr::new("mpv"))
}

fn needs_allanime_referer(stream_url: &str) -> bool {
    stream_url.contains("tools.fast4speed.rsvp")
}

fn socket_path(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("animestan-mpv-{pid}.sock"))
}

fn tracker_lock_error() -> io::Error {
    io::Error::other("episode tracker lock poisoned")
}

fn finalize_without_ipc(
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    child: &mut std::process::Child,
) -> Result<(), Box<dyn std::error::Error>> {
    {
        let mut guard = tracker.lock().map_err(|_| tracker_lock_error())?;
        guard.mark_watched(episode_id)?;
    }

    let _ = child.wait();
    Ok(())
}

#[cfg(unix)]
fn mark_watched_blocking(
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut guard = tracker.lock().map_err(|_| tracker_lock_error())?;
    guard.mark_watched(episode_id)?;
    Ok(())
}

#[cfg(unix)]
fn update_progress_blocking(
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    position: f64,
    duration: Option<f64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut guard = tracker.lock().map_err(|_| tracker_lock_error())?;
    guard.update_progress(episode_id, position, duration)?;
    Ok(())
}

#[cfg(unix)]
fn connect_with_retry(path: &PathBuf, attempts: usize, delay_ms: u64) -> Option<UnixStream> {
    for _ in 0..attempts {
        match UnixStream::connect(path) {
            Ok(stream) => return Some(stream),
            Err(_) => thread::sleep(Duration::from_millis(delay_ms)),
        }
    }
    None
}

#[cfg(unix)]
fn observe_property(stream: &mut UnixStream, id: u32, name: &str) -> io::Result<()> {
    let payload = json!({
        "command": ["observe_property", id, name],
    });
    let bytes = serde_json::to_vec(&payload).map_err(io::Error::other)?;
    stream.write_all(&bytes)?;
    stream.write_all(b"\n")?;
    stream.flush()
}

#[cfg(unix)]
fn ipc_loop(stream: UnixStream, tracker: &Arc<Mutex<EpisodeTracker>>, episode_id: &str) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut duration: Option<f64> = None;
    let mut marked = false;

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    handle_ipc_event(&value, tracker, episode_id, &mut duration, &mut marked);
                }
            }
        }
    }
}

#[cfg(unix)]
fn handle_ipc_event(
    value: &Value,
    tracker: &Arc<Mutex<EpisodeTracker>>,
    episode_id: &str,
    duration: &mut Option<f64>,
    marked: &mut bool,
) {
    let Some(event_name) = value.get("event").and_then(Value::as_str) else {
        return;
    };

    match event_name {
        "property-change" => {
            let Some(property) = value.get("name").and_then(Value::as_str) else {
                return;
            };
            match property {
                "time-pos" => {
                    if let Some(position) = value.get("data").and_then(Value::as_f64) {
                        if let Err(err) =
                            update_progress_blocking(tracker, episode_id, position, *duration)
                        {
                            eprintln!("failed to update progress for '{episode_id}': {err}");
                        }

                        if let Some(total) = *duration {
                            if total > 0.0 && position / total >= WATCHED_THRESHOLD && !*marked {
                                if let Err(err) = mark_watched_blocking(tracker, episode_id) {
                                    eprintln!(
                                        "failed to mark episode '{episode_id}' as watched: {err}"
                                    );
                                } else {
                                    *marked = true;
                                }
                            }
                        }
                    }
                }
                "duration" => {
                    *duration = value.get("data").and_then(Value::as_f64);
                }
                "eof-reached" => {
                    let reached = value.get("data").and_then(Value::as_bool).unwrap_or(false);
                    if reached && !*marked {
                        if let Err(err) = mark_watched_blocking(tracker, episode_id) {
                            eprintln!("failed to mark episode '{episode_id}' as watched: {err}");
                        } else {
                            *marked = true;
                        }
                    }
                }
                _ => {}
            }
        }
        "end-file" => {
            let reason = value.get("reason").and_then(Value::as_str);
            if reason != Some("error") && !*marked {
                if let Err(err) = mark_watched_blocking(tracker, episode_id) {
                    eprintln!("failed to mark episode '{episode_id}' as watched: {err}");
                } else {
                    *marked = true;
                }
            }
        }
        _ => {}
    }
}
