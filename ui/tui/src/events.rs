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

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};

use crate::app::{App, ConfirmExitChoice, Focus, InputMode};

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
            keys: "/",
            description: "Filter the focused list",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "Tab",
            description: "Cycle focus: Anime → Episode → Search",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "← / →",
            description: "Switch panels or rotate trending carousel",
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
            keys: "b",
            description: "Toggle bookmarks pane",
            mode: InputMode::Normal,
        },
        KeyBinding {
            keys: "g",
            description: "Focus trending carousel",
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

    if app.show_keybindings() {
        app.toggle_keybindings();
        return;
    }

    if app.panel_filter_mode() {
        handle_panel_filter_mode(app, key_event);
        return;
    }

    match app.input_mode() {
        InputMode::Normal => handle_normal_mode(app, key_event),
        InputMode::Search => handle_search_mode(app, key_event),
    }
}

fn handle_normal_mode(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Char('s') => app.enter_search_mode(),
        KeyCode::Char('/') => {
            app.enter_panel_filter(app.filter_target_for_focus());
        }
        KeyCode::Char('q') => app.request_exit(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Left | KeyCode::Right => {
            if matches!(app.focus(), Focus::Trending) {
                app.rotate_trending(matches!(key_event.code, KeyCode::Right));
            } else {
                app.toggle_focus();
            }
        }
        KeyCode::Tab => app.cycle_focus(),
        KeyCode::Char('b') => app.toggle_bookmarks_mode(),
        KeyCode::Char('g') => app.focus_trending(),
        KeyCode::Char('m') => app.request_bookmark_toggle(),
        KeyCode::Char('f') => app.cycle_filter(),
        KeyCode::Char('d') => app.request_download(),
        KeyCode::Char('D') => app.request_delete(),
        KeyCode::Char(' ') => app.select_current(),
        KeyCode::Char('?') => app.show_help(),
        KeyCode::Enter => handle_enter_in_normal_mode(app),
        _ => {}
    }
}

fn handle_enter_in_normal_mode(app: &mut App) {
    match app.focus() {
        Focus::Trending => {
            if let Some(entry) = app.trending_entry() {
                app.set_details(entry.detail_summary());
            }
        }
        Focus::Left => app.toggle_focus(),
        Focus::Right => app.request_play(),
    }
}

fn handle_panel_filter_mode(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Enter => app.exit_panel_filter(),
        KeyCode::Backspace => {
            let mut query = app.panel_filter_query().to_string();
            query.pop();
            app.update_panel_filter_query(query);
        }
        KeyCode::Char(ch) => {
            if !key_event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                let mut query = app.panel_filter_query().to_string();
                query.push(ch);
                app.update_panel_filter_query(query);
            }
        }
        _ => {}
    }
}

fn handle_search_mode(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc => app.exit_search_mode(),
        KeyCode::Enter => {
            app.exit_search_mode();
            app.request_search();
            if !matches!(app.focus(), Focus::Left) {
                app.toggle_focus();
            }
        }
        KeyCode::Backspace => {
            app.pop_search_char();
        }
        KeyCode::Char(ch) => {
            if !key_event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                app.append_search_char(ch);
            }
        }
        _ => {}
    }
}
