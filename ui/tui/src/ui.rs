// Copyright (C) 2026 M.R. Siavash Katebzadeg <mr@katebzadeh.xyz>
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
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Focus, InputMode, LeftPaneMode};

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(5),
        ])
        .split(frame.area());

    render_search_bar(frame, chunks[0], app);

    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let (left_title, anime_items) = match app.left_pane_mode() {
        LeftPaneMode::Search => ("Anime", build_anime_items(app)),
        LeftPaneMode::Bookmarks => ("Bookmarks", build_bookmark_items(app)),
    };
    render_list(
        frame,
        lists[0],
        left_title,
        anime_items,
        app.left_index(),
        app.focus() == Focus::Left,
    );

    let episode_items = build_episode_items(app);
    let episodes_title = if let Some(label) = app.filter_label() {
        format!("Episodes [{label}]")
    } else {
        "Episodes".to_string()
    };
    render_list(
        frame,
        lists[1],
        &episodes_title,
        episode_items,
        app.right_index(),
        app.focus() == Focus::Right,
    );

    render_details(frame, chunks[2], app);
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
    let details = Paragraph::new(lines)
        .block(details_block)
        .wrap(Wrap { trim: true });
    frame.render_widget(details, area);
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
            ListItem::new(format!("{marker} {}", entry.title))
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
            ListItem::new(format!("{marker} {}", entry.anime.title))
        })
        .collect()
}

fn build_episode_items(app: &App) -> Vec<ListItem<'_>> {
    app.episodes()
        .iter()
        .enumerate()
        .map(|(idx, episode)| {
            let marker = if Some(idx) == app.selected_episode() {
                '★'
            } else {
                ' '
            };
            ListItem::new(format!(
                "{marker} {:>03} — {}",
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
