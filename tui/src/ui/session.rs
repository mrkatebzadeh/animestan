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

use crate::app::App;
use crate::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use throbber_widgets_tui::Throbber;

use super::border_style;

pub(super) fn render_session_panel(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(theme, false));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let pane_label = "Anime";
    let filter_label = app.filter_label().unwrap_or("All");
    let mut lines = vec![Line::from(app.details())];
    lines.push(Line::from(format!(
        "Pane: {pane_label} | Filter: {filter_label}"
    )));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let details = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(details, chunks[0]);

    let now_playing_label = app
        .current_playback_label()
        .unwrap_or_else(|| "Idle".to_string());
    let elapsed = format_elapsed(app.playback_elapsed());
    let spans = vec![
        Span::styled(now_playing_label, theme.title_style()),
        Span::raw(" | "),
        Span::styled(format!("Elapsed: {elapsed}"), theme.item_style()),
    ];
    let now_playing = Paragraph::new(Line::from(spans)).wrap(Wrap { trim: true });
    frame.render_widget(now_playing, chunks[1]);

    let left_status = format!(
        "Mode: {} | Selection: {}",
        app.mode_label(),
        app.current_selection_label(),
    );
    let hint_text = "Press ? for keybindings";
    let hint_len = u16::try_from(hint_text.chars().count()).unwrap_or(u16::MAX);
    let throbber_width = 2u16;

    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(throbber_width),
            Constraint::Length(hint_len),
        ])
        .split(chunks[2]);

    let status = Paragraph::new(Line::from(Span::styled(
        left_status,
        theme.item_style().bg(theme.non_interactive_color()),
    )))
    .style(Style::default().bg(theme.non_interactive_color()));
    frame.render_widget(status, status_chunks[0]);

    let hint = Paragraph::new(Line::from(Span::styled(
        hint_text,
        Style::default()
            .bg(theme.non_interactive_color())
            .add_modifier(Modifier::REVERSED),
    )))
    .style(Style::default().bg(theme.non_interactive_color()));
    frame.render_widget(hint, status_chunks[2]);

    let throbber_bg = Paragraph::new(" ").style(Style::default().bg(theme.non_interactive_color()));
    frame.render_widget(throbber_bg, status_chunks[1]);
    if app.background_refreshing() {
        let throbber = Throbber::default();
        frame.render_stateful_widget(throbber, status_chunks[1], app.metadata_throbber_mut());
    }
}

fn format_elapsed(seconds: Option<f64>) -> String {
    let secs = seconds.unwrap_or(0.0).max(0.0);
    let duration = std::time::Duration::from_secs_f64(secs);
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}
