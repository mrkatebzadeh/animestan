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

use std::convert::TryFrom;

use crate::app::{App, FilterTarget, Focus};
use crate::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Frame;
use ratatui::style::Style;

mod details;
mod heatmap;
mod lists;
mod modals;
mod preview;
mod search;
mod session;

use self::details::render_anime_details_panel;
use self::heatmap::render_episode_heatmap;
use self::lists::{build_episode_items, render_anime_table, render_list};
use self::modals::{
    render_exit_confirmation_modal, render_info_modal, render_keybindings_modal,
    render_quick_launch_palette, render_search_results_modal,
};
use self::search::{render_panel_filter_input, should_show_panel_filter, split_filter_area};
use self::session::render_session_panel;

pub fn render(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let frame_area = frame.area();
    let total_height = frame_area.height as usize;
    let session_default = 7;
    let session_min = 3;
    let progress_panel_height = 3;
    let available_for_session = total_height.saturating_sub(progress_panel_height + session_min);
    let session_height_usize = available_for_session.min(session_default);
    let max_progress_height = total_height.saturating_sub(session_height_usize).max(1);
    let heatmap_height = progress_panel_height.min(max_progress_height);
    let heatmap_length = u16::try_from(heatmap_height.min(u16::MAX as usize)).unwrap_or(u16::MAX);
    let list_height = total_height.saturating_sub(session_height_usize + heatmap_height);
    let list_length = u16::try_from(list_height.min(u16::MAX as usize)).unwrap_or(u16::MAX);
    let session_length =
        u16::try_from(session_height_usize.min(u16::MAX as usize)).unwrap_or(u16::MAX);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(list_length),
            Constraint::Length(heatmap_length),
            Constraint::Length(session_length),
        ])
        .split(frame_area);

    let lists_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let details_height = 9u16;
    let left_column_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(details_height)])
        .split(lists_layout[0]);
    let anime_panel_area = left_column_split[0];
    let details_panel_area = left_column_split[1];

    let left_target = FilterTarget::Anime;
    let left_filter_visible = should_show_panel_filter(app, left_target);
    let (left_filter_area, left_list_area) =
        split_filter_area(anime_panel_area, left_filter_visible);
    if let Some(area) = left_filter_area {
        render_panel_filter_input(frame, area, app, left_filter_visible, theme);
    }
    render_anime_table(frame, left_list_area, app, theme);
    render_anime_details_panel(frame, details_panel_area, app, theme);

    let episode_items = build_episode_items(app);
    let mut episodes_title = if let Some(label) = app.filter_label() {
        format!("Episodes [{label}]")
    } else {
        "Episodes".to_string()
    };
    if app.panel_filter_active_for(FilterTarget::Episodes) {
        episodes_title.push_str(" [Filtered]");
    }
    let right_filter_visible = should_show_panel_filter(app, FilterTarget::Episodes);
    let (right_filter_area, right_list_area) =
        split_filter_area(lists_layout[1], right_filter_visible);
    if let Some(area) = right_filter_area {
        render_panel_filter_input(frame, area, app, right_filter_visible, theme);
    }
    render_list(
        frame,
        right_list_area,
        &episodes_title,
        episode_items,
        app.right_index(),
        app.focus() == Focus::Right,
        theme,
    );

    render_episode_heatmap(frame, chunks[1], app, theme);
    render_session_panel(frame, chunks[2], app, theme);
    render_search_results_modal(frame, app, theme);
    render_keybindings_modal(frame, app, theme);
    render_info_modal(frame, app, theme);
    render_exit_confirmation_modal(frame, app, theme);
    render_quick_launch_palette(frame, app, theme);
}

pub(super) fn border_style(theme: &Theme, focused: bool) -> Style {
    theme.panel_border_style(focused)
}
