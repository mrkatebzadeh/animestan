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

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

pub(super) fn handle_modal(app: &mut App, key_event: KeyEvent) -> bool {
    if !app.show_keybindings() {
        return false;
    }

    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.toggle_keybindings();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.scroll_keybindings(1);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.scroll_keybindings(-1);
        }
        KeyCode::PageDown => {
            let viewport = app.keybindings_viewport_lines();
            let step = i64::try_from(viewport.max(1)).unwrap_or(i64::MAX);
            app.scroll_keybindings(step);
        }
        KeyCode::PageUp => {
            let viewport = app.keybindings_viewport_lines();
            let step = i64::try_from(viewport.max(1)).unwrap_or(i64::MAX);
            app.scroll_keybindings(-step);
        }
        KeyCode::Home => {
            app.set_keybindings_scroll(0);
        }
        KeyCode::End => {
            app.set_keybindings_scroll(app.keybindings_max_scroll());
        }
        _ => {}
    }

    true
}
