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

pub(super) fn handle(app: &mut App, key_event: KeyEvent) {
    let candidates_len = app.quick_launch_items().len();
    match key_event.code {
        KeyCode::Esc => app.close_quick_launch(),
        KeyCode::Enter => app.run_quick_launch_selection(),
        KeyCode::Backspace => app.pop_quick_launch_char(),
        KeyCode::Down | KeyCode::Char('j') => {
            app.move_quick_launch_selection_down(candidates_len);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_quick_launch_selection_up();
        }
        KeyCode::Char(ch) => {
            if !key_event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                app.append_quick_launch_char(ch);
            }
        }
        _ => {}
    }
}
