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

use std::collections::HashSet;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind};

use crate::app::{App, ConfirmExitChoice, InputMode};

mod keybindings;
mod normal;
mod panel_filter;
mod quick_launch;
mod search_results;

#[derive(Clone, Copy, Debug)]
pub struct KeyBinding {
    pub keys: &'static str,
    pub description: &'static str,
    pub mode: InputMode,
}

#[allow(clippy::too_many_lines)]
pub fn keybindings() -> &'static [KeyBinding] {
    static BINDINGS: &[KeyBinding] = &[
        KeyBinding {
            keys: "?",
            description: "Toggle keybindings modal",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "s",
            description: "Search for anime",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Ctrl+K",
            description: "Open quick launch palette",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Ctrl+M",
            description: "Mark highlighted search result",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "w",
            description: "Mark current episode as watched",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "u",
            description: "Mark current episode as unwatched",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "W",
            description: "Mark all episodes as watched",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "U",
            description: "Mark all episodes as unwatched",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "K",
            description: "Mark episodes up to current as watched",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "/",
            description: "Filter the focused list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Tab",
            description: "Cycle focus: Anime ↔ Episode",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "← / →",
            description: "Switch between anime and episodes",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "j / ↓",
            description: "Move selection down",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "k / ↑",
            description: "Move selection up",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "gg",
            description: "Jump to top of focused list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "G",
            description: "Jump to bottom of focused list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "M",
            description: "Jump to middle of focused list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Ctrl+D",
            description: "Half-page down list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Ctrl+U",
            description: "Half-page up list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "i",
            description: "Show anime info",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "m",
            description: "Toggle bookmark for selection",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "f",
            description: "Cycle episode filters",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Space",
            description: "Select focused item",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Enter",
            description: "Play episode / move focus",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "d",
            description: "Download highlighted episode",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "D",
            description: "Delete downloaded episode",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "q",
            description: "Quit the app",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Esc",
            description: "Cancel search / input",
            mode: InputMode::Search,
        },
        KeyBinding {
            keys: "Enter",
            description: "Submit search query",
            mode: InputMode::Search,
        },
        KeyBinding {
            keys: "Backspace",
            description: "Delete last character",
            mode: InputMode::Search,
        },
        KeyBinding {
            keys: "Text",
            description: "Append to search query",
            mode: InputMode::Search,
        },
    ];

    BINDINGS
}

pub enum Event {
    Tick,
    Input(KeyEvent),
}

pub struct EventHandler {
    tick_rate: Duration,
    last_tick: Instant,
    pressed_keys: HashSet<KeyCode>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self {
            tick_rate,
            last_tick: Instant::now(),
            pressed_keys: HashSet::new(),
        }
    }

    pub fn next(&mut self) -> io::Result<Event> {
        let timeout = self.tick_rate.saturating_sub(self.last_tick.elapsed());

        if event::poll(timeout)? {
            if let CrosstermEvent::Key(key_event) = event::read()? {
                let code = key_event.code;

                match key_event.kind {
                    KeyEventKind::Press => {
                        self.pressed_keys.insert(code);
                        return Ok(Event::Input(key_event));
                    }
                    KeyEventKind::Repeat => {
                        if self.pressed_keys.contains(&code) {
                            return Ok(Event::Input(key_event));
                        }
                    }
                    KeyEventKind::Release => {
                        self.pressed_keys.remove(&code);
                    }
                }
            }
        }

        if self.last_tick.elapsed() >= self.tick_rate {
            self.last_tick = Instant::now();
        }

        Ok(Event::Tick)
    }
}

pub fn handle_key_event(app: &mut App, key_event: KeyEvent) {
    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return;
    }

    if app.confirm_exit() {
        match key_event.code {
            KeyCode::Left | KeyCode::Char('h') => {
                app.set_confirm_exit_choice(ConfirmExitChoice::Yes);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                app.set_confirm_exit_choice(ConfirmExitChoice::No);
            }
            KeyCode::Tab => {
                app.toggle_confirm_exit_choice();
            }
            KeyCode::Enter => match app.confirm_exit_choice() {
                ConfirmExitChoice::Yes => app.confirm_exit_and_quit(),
                ConfirmExitChoice::No => {
                    app.set_confirm_exit_choice(ConfirmExitChoice::No);
                    app.clear_confirm_exit();
                }
            },
            KeyCode::Char('y') => {
                app.set_confirm_exit_choice(ConfirmExitChoice::Yes);
                app.confirm_exit_and_quit();
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                app.set_confirm_exit_choice(ConfirmExitChoice::No);
                app.clear_confirm_exit();
            }
            _ => app.clear_confirm_exit(),
        }
        return;
    }

    if app.info_modal_visible() {
        match key_event.code {
            KeyCode::Esc => app.close_info_modal(),
            KeyCode::Char('q') => app.request_exit(),
            _ => {}
        }
        return;
    }

    if app.search_results_modal_visible() && search_results::handle_modal(app, key_event) {
        return;
    }

    if app.show_keybindings() && keybindings::handle_modal(app, key_event) {
        return;
    }

    if app.quick_launch_active() {
        quick_launch::handle(app, key_event);
        return;
    }

    if app.panel_filter_mode() {
        panel_filter::handle(app, key_event);
        return;
    }

    normal::handle(app, key_event);
}
