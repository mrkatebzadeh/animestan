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
use std::time::{Duration, Instant};

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};

use crate::app::{App, InputMode};

pub enum Event {
    Tick,
    Input(KeyEvent),
}

pub struct EventHandler {
    tick_rate: Duration,
    last_tick: Instant,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self {
            tick_rate,
            last_tick: Instant::now(),
        }
    }

    pub fn next(&mut self) -> io::Result<Event> {
        let timeout = self.tick_rate.saturating_sub(self.last_tick.elapsed());

        if event::poll(timeout)? {
            if let CrosstermEvent::Key(key_event) = event::read()? {
                return Ok(Event::Input(key_event));
            }
        }

        if self.last_tick.elapsed() >= self.tick_rate {
            self.last_tick = Instant::now();
        }

        Ok(Event::Tick)
    }
}

pub fn handle_key_event(app: &mut App, key_event: KeyEvent) {
    if key_event.kind != KeyEventKind::Press {
        return;
    }

    match app.input_mode() {
        InputMode::Normal => handle_normal_mode(app, key_event),
        InputMode::Search => handle_search_mode(app, key_event),
    }
}

fn handle_normal_mode(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Char('/') => app.enter_search_mode(),
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('h' | 'l') | KeyCode::Left | KeyCode::Right => {
            app.toggle_focus();
        }
        KeyCode::Char('b') => app.toggle_bookmarks_mode(),
        KeyCode::Char('f') => app.cycle_filter(),
        KeyCode::Char('d') => app.request_download(),
        KeyCode::Char('D') => app.request_delete(),
        KeyCode::Char(' ') => app.select_current(),
        KeyCode::Char('?') => app.show_help(),
        KeyCode::Enter => app.request_play(),
        _ => {}
    }
}

fn handle_search_mode(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc => app.exit_search_mode(),
        KeyCode::Enter => {
            app.exit_search_mode();
            app.request_search();
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
