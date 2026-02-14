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

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, FilterTarget, Focus, InputMode, LeftPaneMode};
use crate::events::keybindings;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(4),
        ])
        .split(frame.area());

    render_search_bar(frame, chunks[0], app);

    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let (left_base_title, anime_items) = match app.left_pane_mode() {
        LeftPaneMode::Search => ("Anime", build_anime_items(app)),
        LeftPaneMode::Bookmarks => ("Bookmarks", build_bookmark_items(app)),
    };
    let left_target = match app.left_pane_mode() {
        LeftPaneMode::Search => FilterTarget::Anime,
        LeftPaneMode::Bookmarks => FilterTarget::Bookmarks,
    };
    let mut left_title = left_base_title.to_string();
    if app.panel_filter_active_for(left_target) {
        left_title.push_str(" [Filtered]");
    }
    let left_filter_visible = should_show_panel_filter(app, left_target);
    let (left_filter_area, left_list_area) = split_filter_area(lists[0], left_filter_visible);
    if let Some(area) = left_filter_area {
        render_panel_filter_input(frame, area, app, left_filter_visible);
    }
    render_list(
        frame,
        left_list_area,
        &left_title,
        anime_items,
        app.left_index(),
        app.focus() == Focus::Left,
    );

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
    let (right_filter_area, right_list_area) = split_filter_area(lists[1], right_filter_visible);
    if let Some(area) = right_filter_area {
        render_panel_filter_input(frame, area, app, right_filter_visible);
    }
    render_list(
        frame,
        right_list_area,
        &episodes_title,
        episode_items,
        app.right_index(),
        app.focus() == Focus::Right,
    );

    render_details(frame, chunks[2], app);
    render_keybindings_modal(frame, app);
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem>,
    active_index: usize,
    focused: bool,
) {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style(focused));

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(active_index.min(items.len() - 1)));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state);
}

fn split_filter_area(area: Rect, show_filter: bool) -> (Option<Rect>, Rect) {
    if !show_filter {
        return (None, area);
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    (Some(chunks[0]), chunks[1])
}

fn render_panel_filter_input(frame: &mut Frame, area: Rect, app: &App, active: bool) {
    let block = Block::default()
        .title("Filter")
        .borders(Borders::ALL)
        .border_style(border_style(active));
    let prompt = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::DarkGray)),
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

fn should_show_panel_filter(app: &App, target: FilterTarget) -> bool {
    app.panel_filter_mode() && app.panel_filter_target() == Some(target)
}

fn render_search_bar(frame: &mut Frame, area: Rect, app: &App) {
    let title = match app.input_mode() {
        InputMode::Normal => "Mode: Normal",
        InputMode::Search => "Mode: Search",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style(app.input_mode() == InputMode::Search));

    let prompt = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::DarkGray)),
        Span::raw(app.search_query()),
    ]);

    let inner = block.inner(area);
    let paragraph = Paragraph::new(prompt).block(block);
    frame.render_widget(paragraph, area);

    if app.input_mode() == InputMode::Search {
        let typed_chars = app.search_query().chars().count();
        let typed_offset = u16::try_from(typed_chars).unwrap_or(u16::MAX);
        let cursor_base = inner.x.saturating_add(2);
        let max_cursor = inner.x.saturating_add(inner.width.saturating_sub(1));
        let cursor_x = cursor_base.saturating_add(typed_offset).min(max_cursor);
        frame.set_cursor_position((cursor_x, inner.y));
    }
}

fn render_details(frame: &mut Frame, area: Rect, app: &App) {
    let details_block = Block::default()
        .title("Details")
        .borders(Borders::ALL)
        .border_style(border_style(false));
    let pane_label = match app.left_pane_mode() {
        LeftPaneMode::Search => "Search",
        LeftPaneMode::Bookmarks => "Bookmarks",
    };
    let filter_label = app.filter_label().unwrap_or("All");
    let mut lines = vec![Line::from(app.details())];
    lines.push(Line::from(format!(
        "Pane: {pane_label} | Filter: {filter_label}"
    )));
    if matches!(app.focus(), Focus::Right) && !app.episodes().is_empty() {
        lines.push(Line::from("Hint: d download | D delete local copy"));
    }
    let left_status = format!(
        "{} | {} | {}",
        app.mode_label(),
        app.current_selection_label(),
        app.playback_status().label()
    );
    let inner_area = details_block.inner(area);
    frame.render_widget(details_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner_area);

    let details = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(details, chunks[0]);

    let status_line = Line::from(Span::styled(
        left_status,
        Style::default().fg(Color::White).bg(Color::DarkGray),
    ));
    let status = Paragraph::new(status_line).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(status, chunks[1]);
}

fn build_anime_items(app: &App) -> Vec<ListItem<'_>> {
    app.anime_entries()
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let marker = if Some(idx) == app.selected_anime() {
                '★'
            } else {
                ' '
            };
            let bookmark_icon = if app.is_bookmarked(entry.id.as_str()) {
                '★'
            } else {
                ' '
            };
            ListItem::new(format!("{marker} {bookmark_icon} {}", entry.title))
        })
        .collect()
}

fn build_bookmark_items(app: &App) -> Vec<ListItem<'_>> {
    app.bookmark_entries()
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let marker = if Some(idx) == app.selected_anime() {
                '★'
            } else {
                ' '
            };
            ListItem::new(format!("{marker} ★ {}", entry.anime.title))
        })
        .collect()
}

fn build_episode_items(app: &App) -> Vec<ListItem<'_>> {
    if app.episodes_loading() {
        return vec![ListItem::new("Fetching episodes...")];
    }

    app.episodes()
        .iter()
        .enumerate()
        .map(|(idx, episode)| {
            let marker = if Some(idx) == app.selected_episode() {
                '★'
            } else {
                ' '
            };
            let indicators = app.episode_indicators(&episode.id);
            let play_icon = if indicators.watched {
                "✔"
            } else if indicators.in_progress {
                "◔"
            } else {
                "○"
            };
            let download_icon = if indicators.downloaded { "💾" } else { "⬇" };
            ListItem::new(format!(
                "{marker} {play_icon} {download_icon} {:>03} — {}",
                episode.number, episode.title
            ))
        })
        .collect()
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn render_keybindings_modal(frame: &mut Frame, app: &App) {
    if !app.show_keybindings() {
        return;
    }

    let bindings = keybindings();
    if bindings.is_empty() {
        return;
    }

    let key_width = bindings
        .iter()
        .map(|binding| binding.keys.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);

    let mut lines = Vec::new();
    let mut current_mode: Option<InputMode> = None;
    for binding in bindings {
        if current_mode != Some(binding.mode) {
            if current_mode.is_some() {
                lines.push(Line::default());
            }
            current_mode = Some(binding.mode);
            lines.push(Line::from(Span::styled(
                format!("{} mode", input_mode_label(binding.mode)),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let key_label = format!("{:<width$}", binding.keys, width = key_width);
        lines.push(Line::from(vec![
            Span::styled(key_label, Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::raw(binding.description),
        ]));
    }

    let frame_area = frame.area();
    let width = frame_area.width.saturating_sub(4).min(80);
    let max_height = frame_area.height.saturating_sub(4);
    if width == 0 || max_height == 0 {
        return;
    }
    let content_height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let mut height = content_height.saturating_add(4);
    if height > max_height {
        height = max_height;
    }

    let area = centered_rect(frame_area, width, height);
    let block = Block::default().title("Keybindings").borders(Borders::ALL);
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let capped_width = width.min(area.width);
    let capped_height = height.min(area.height);
    let offset_x = area.x + area.width.saturating_sub(capped_width) / 2;
    let offset_y = area.y + area.height.saturating_sub(capped_height) / 2;
    Rect::new(offset_x, offset_y, capped_width, capped_height)
}

fn input_mode_label(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "Normal",
        InputMode::Search => "Search",
    }
}
