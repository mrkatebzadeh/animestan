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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

pub(super) fn handle_modal(app: &mut App, key_event: KeyEvent) -> bool {
    if handle_navigation_shortcuts(app, key_event) {
        return true;
    }

    match key_event.code {
        KeyCode::Esc => app.close_search_results_modal(),
        KeyCode::Enter => app.request_search_results_add(),
        KeyCode::Down | KeyCode::Char('j') => app.move_search_results_selection_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_search_results_selection_up(),
        KeyCode::Char('m') => {
            if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                app.request_bookmark_toggle();
            }
        }
        _ => {}
    }

    true
}

fn handle_navigation_shortcuts(app: &mut App, key_event: KeyEvent) -> bool {
    if app.search_results().is_empty() {
        return false;
    }

    if matches!(key_event.code, KeyCode::Char('g')) && key_event.modifiers.is_empty() {
        if app.consume_pending_double_g() {
            app.search_results_move_to_top();
        } else {
            app.start_pending_double_g();
        }
        return true;
    }

    app.cancel_pending_double_g();

    match key_event.code {
        KeyCode::Char('G') => {
            app.search_results_move_to_bottom();
            true
        }
        KeyCode::Char('d' | 'D') => {
            if key_event.modifiers.intersects(KeyModifiers::CONTROL) {
                app.search_results_half_page_down();
                true
            } else {
                false
            }
        }
        KeyCode::Char('u' | 'U') => {
            if key_event.modifiers.intersects(KeyModifiers::CONTROL) {
                app.search_results_half_page_up();
                true
            } else {
                false
            }
        }
        _ => false,
    }
}
