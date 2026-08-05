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

use crate::app::{App, Focus};

pub(super) fn handle(app: &mut App, key_event: KeyEvent) {
    if handle_navigation_shortcuts(app, key_event) {
        return;
    }

    match key_event.code {
        KeyCode::Char('s') => app.enter_search_mode(),
        KeyCode::Char('/') => {
            app.enter_panel_filter(app.filter_target_for_focus());
        }
        KeyCode::Char('q') => app.request_exit(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            app.open_quick_launch();
        }
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Left | KeyCode::Right => {
            app.toggle_focus();
        }
        KeyCode::Tab => app.cycle_focus(),
        KeyCode::Char('w') => app.request_mark_current_episode(true),
        KeyCode::Char('u') => app.request_mark_current_episode(false),
        KeyCode::Char('m') => app.request_bookmark_toggle(),
        KeyCode::Char('W') => app.request_mark_all_episodes(true),
        KeyCode::Char('U') => app.request_mark_all_episodes(false),
        KeyCode::Char('K') => app.request_mark_up_to_current(),
        KeyCode::Char('f') => app.cycle_filter(),
        KeyCode::Char('R') => app.request_current_anime_refresh(),
        KeyCode::Char('i') => {
            app.open_info_modal();
            app.set_details("Press Esc to close info modal.");
        }
        KeyCode::Char('d') => app.request_download(),
        KeyCode::Char('D') => app.request_delete(),
        KeyCode::Char(' ') => app.select_current(),
        KeyCode::Char('?') => app.toggle_keybindings(),
        KeyCode::Enter => handle_enter(app),
        _ => {}
    }
}

fn handle_enter(app: &mut App) {
    if matches!(app.focus(), Focus::Left) {
        app.toggle_focus();
    } else {
        app.request_play_async();
    }
}

fn handle_navigation_shortcuts(app: &mut App, key_event: KeyEvent) -> bool {
    if matches!(key_event.code, KeyCode::Char('g')) && key_event.modifiers.is_empty() {
        if app.consume_pending_double_g() {
            app.move_to_top();
        } else {
            app.start_pending_double_g();
        }
        return true;
    }

    app.cancel_pending_double_g();

    match key_event.code {
        KeyCode::Char('G') => {
            app.move_to_bottom();
            true
        }
        KeyCode::Char('M') => {
            app.move_to_middle();
            true
        }
        KeyCode::Char('d' | 'D') if key_event.modifiers.intersects(KeyModifiers::CONTROL) => {
            app.half_page_down();
            true
        }
        KeyCode::Char('u' | 'U') if key_event.modifiers.intersects(KeyModifiers::CONTROL) => {
            app.half_page_up();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use animestan_core::{AnimeEntry, FavoriteStore};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_path() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should advance")
            .as_nanos();
        let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "animestan-normal-test-{}-{stamp}-{counter}.json",
            std::process::id(),
        ))
    }

    fn app_with_bookmark() -> App {
        let mut store = FavoriteStore::load(unique_temp_path()).expect("store should load");
        store
            .add(AnimeEntry {
                id: "naruto".to_string(),
                title: "Naruto".to_string(),
                source_id: "anidb".to_string(),
            })
            .expect("bookmark should persist");

        let mut app = App::new();
        app.load_bookmarks(&store);
        app
    }

    #[test]
    fn temp_paths_are_unique_when_requested_repeatedly() {
        assert_ne!(unique_temp_path(), unique_temp_path());
    }

    #[test]
    fn pressing_shift_r_requests_highlighted_anime_refresh() {
        let mut app = app_with_bookmark();

        handle(
            &mut app,
            KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE),
        );

        let refresh = app
            .take_pending_anime_refresh()
            .expect("refresh request should be queued");
        assert_eq!(refresh.anime_id, "naruto");
        assert_eq!(refresh.title, "Naruto");
    }
}
