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

use crate::app::{App, FilterTarget};
use crate::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::border_style;

pub(super) fn split_filter_area(area: Rect, show_filter: bool) -> (Option<Rect>, Rect) {
    if !show_filter {
        return (None, area);
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    (Some(chunks[0]), chunks[1])
}

pub(super) fn render_panel_filter_input(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    active: bool,
    theme: &Theme,
) {
    let block = Block::default()
        .title("Filter")
        .borders(Borders::ALL)
        .border_style(border_style(theme, active));
    let prompt = Line::from(vec![
        Span::styled("> ", theme.non_interactive_style()),
        Span::raw(app.panel_filter_query()),
    ]);
    let inner = block.inner(area);
    let paragraph = Paragraph::new(prompt).block(block);
    frame.render_widget(paragraph, area);

    if active {
        let typed_chars = app.panel_filter_query().chars().count();
        let typed_offset = u16::try_from(typed_chars).unwrap_or(u16::MAX);
        let cursor_base = inner.x.saturating_add(2);
        let max_cursor = inner.x.saturating_add(inner.width.saturating_sub(1));
        let cursor_x = cursor_base.saturating_add(typed_offset).min(max_cursor);
        frame.set_cursor_position((cursor_x, inner.y));
    }
}

pub(super) fn should_show_panel_filter(app: &App, target: FilterTarget) -> bool {
    app.panel_filter_mode() && app.panel_filter_target() == Some(target)
}
