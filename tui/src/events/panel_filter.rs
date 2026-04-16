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
    match key_event.code {
        KeyCode::Esc | KeyCode::Enter => app.exit_panel_filter(),
        KeyCode::Backspace => {
            let mut query = app.panel_filter_query().to_string();
            query.pop();
            app.update_panel_filter_query(query);
        }
        KeyCode::Char(ch)
            if !key_event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            let mut query = app.panel_filter_query().to_string();
            query.push(ch);
            app.update_panel_filter_query(query);
        }
        _ => {}
    }
}
