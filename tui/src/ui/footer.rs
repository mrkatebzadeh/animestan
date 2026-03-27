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

use crate::app::{App, PlaybackStatus};
use crate::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use throbber_widgets_tui::Throbber;

const FOOTER_KEYBINDINGS: [(&str, &str); 10] = [
    ("s", "search"),
    ("?", "keybindings"),
    ("j/k", "move"),
    ("h/l", "focus"),
    ("enter/tab", "open"),
    ("/", "filter"),
    ("m", "bookmark"),
    ("w", "mark"),
    ("d", "download"),
    ("q", "quit"),
];

const KEY_COLORS: [Color; 4] = [Color::Cyan, Color::Magenta, Color::Yellow, Color::Green];
const STATUS_PREFIX: &str = "Mode: ";
const STATUS_SEPARATOR: &str = " · ";
const CAP_LEFT: &str = "";
const CAP_RIGHT: &str = "";

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let mut constraints = vec![Constraint::Length(1)];
    if area.height > 1 {
        constraints.push(Constraint::Length(1));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.as_slice())
        .split(area);

    render_footer_status_line(frame, chunks[0], app, theme);
    if chunks.len() > 1 {
        render_footer_keybindings(frame, chunks[1], theme);
    }
}

fn render_footer_status_line(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0)])
        .split(area);

    render_status_text(frame, status_chunks[0], app, theme);
}

fn render_status_text(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let left_status = status_label(app);
    let spinner_span = if app.background_refreshing() {
        let throbber = Throbber::default().throbber_style(
            Style::default()
                .fg(theme.title_color())
                .bg(theme.non_interactive_color()),
        );
        Some(throbber.to_symbol_span(app.metadata_throbber()))
    } else {
        None
    };
    let spinner_width = spinner_span.as_ref().map_or(0, Span::width);

    let cap_width = CAP_LEFT.chars().count() + CAP_RIGHT.chars().count();
    let area_width = area.width as usize;
    let inner_width = area_width.saturating_sub(cap_width + spinner_width);
    let trimmed = trim_to_width(&left_status, inner_width);
    let centered = center_text(&trimmed, inner_width);

    let mut spans = Vec::new();
    spans.push(Span::styled(
        CAP_LEFT,
        Style::default().fg(theme.non_interactive_color()),
    ));
    if let Some(span) = spinner_span {
        spans.push(span);
    }
    if inner_width > 0 {
        spans.push(Span::styled(
            centered,
            Style::default()
                .fg(theme.title_color())
                .bg(theme.non_interactive_color()),
        ));
    }
    spans.push(Span::styled(
        CAP_RIGHT,
        Style::default().fg(theme.non_interactive_color()),
    ));

    let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Reset));
    frame.render_widget(paragraph, area);
}

fn render_footer_keybindings(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    if let Some(line) = build_keybinding_line(area.width as usize, theme) {
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }
}

fn build_keybinding_line(width: usize, theme: &Theme) -> Option<Line<'static>> {
    if width == 0 {
        return None;
    }

    let mut selected_count = 0;
    let mut chunk_width = 0;
    for candidate in (1..=FOOTER_KEYBINDINGS.len()).rev() {
        let chunk = width / candidate;
        if chunk == 0 {
            continue;
        }
        let max_len = FOOTER_KEYBINDINGS[..candidate]
            .iter()
            .map(|(key, action)| key.len() + 1 + action.len())
            .max()
            .unwrap_or(0);
        if max_len <= chunk {
            selected_count = candidate;
            chunk_width = chunk;
            break;
        }
    }

    if selected_count == 0 {
        return None;
    }

    let remainder = width - (chunk_width * selected_count);
    let mut spans = Vec::new();
    for (idx, &(key, action)) in FOOTER_KEYBINDINGS[..selected_count].iter().enumerate() {
        let chunk = if idx + 1 == selected_count {
            chunk_width + remainder
        } else {
            chunk_width
        };
        let entry_len = key.len() + 1 + action.len();
        let pad_total = chunk.saturating_sub(entry_len);
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;

        for _ in 0..pad_left {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            key,
            Style::default()
                .fg(KEY_COLORS[idx % KEY_COLORS.len()])
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(":", Style::default().fg(theme.title_color())));
        spans.push(Span::styled(
            action,
            Style::default().fg(theme.non_interactive_color()),
        ));
        for _ in 0..pad_right {
            spans.push(Span::raw(" "));
        }
    }

    Some(Line::from(spans))
}

fn status_label(app: &App) -> String {
    let selection = app.current_selection_label();
    let status = match app.playback_status() {
        PlaybackStatus::Playing => app
            .current_playback_label()
            .unwrap_or_else(|| "Playing".to_string()),
        PlaybackStatus::Downloading => "Downloading".to_string(),
        PlaybackStatus::None => "Idle".to_string(),
    };
    format!(
        "{STATUS_PREFIX}{mode}{STATUS_SEPARATOR}Selection: {selection}{STATUS_SEPARATOR}Status: {status}",
        mode = app.mode_label()
    )
}

fn trim_to_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if value.len() <= width {
        value.to_string()
    } else {
        value.chars().take(width).collect()
    }
}

fn center_text(text: &str, width: usize) -> String {
    if width <= text.chars().count() {
        return text.to_string();
    }
    let total_padding = width - text.chars().count();
    let left_padding = total_padding / 2;
    let right_padding = total_padding - left_padding;
    let mut buffer = String::with_capacity(width);
    for _ in 0..left_padding {
        buffer.push(' ');
    }
    buffer.push_str(text);
    for _ in 0..right_padding {
        buffer.push(' ');
    }
    buffer
}
