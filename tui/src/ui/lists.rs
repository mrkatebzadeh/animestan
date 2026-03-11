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

use crate::app::{App, Focus};
use crate::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::prelude::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::block::Title;
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, TableState,
};

use super::border_style;

pub(super) fn render_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem>,
    active_index: usize,
    focused: bool,
    theme: &Theme,
) {
    let title = if focused {
        Title::from(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        ))
    } else {
        Title::from(title)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .border_style(border_style(theme, focused));

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(active_index.min(items.len() - 1)));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_item_style())
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state);
}

pub(super) fn render_anime_table(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let favorites = app.bookmark_entries();
    if favorites.is_empty() {
        let block = Block::default()
            .title("Anime")
            .borders(Borders::ALL)
            .border_style(border_style(theme, app.focus() == Focus::Left));
        let paragraph = Paragraph::new("No favorites yet. Use the CLI to add some.")
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let rows: Vec<Row> = favorites
        .iter()
        .map(|entry| {
            let progress_stats = app.anime_progress_for(&entry.anime.id);
            let progress = progress_stats.map_or_else(
                || "--/--".to_string(),
                |stats| format!("{}/{}", stats.watched, stats.total),
            );
            let metadata_summary = app.metadata_summary(&entry.anime.id);
            let status = metadata_summary
                .as_ref()
                .and_then(|summary| summary.status.as_deref())
                .unwrap_or("—")
                .to_string();
            let score = metadata_summary
                .as_ref()
                .and_then(|summary| summary.score)
                .map_or_else(|| "—".to_string(), |value| format!("{value:.1}"));
            Row::new(vec![
                Cell::from(entry.anime.title.clone()),
                Cell::from(status),
                Cell::from(score),
                Cell::from(progress),
            ])
        })
        .collect();

    let mut state = TableState::default();
    if !rows.is_empty() {
        state.select(Some(app.left_index().min(rows.len() - 1)));
    }

    let focused = app.focus() == Focus::Left;
    let mut border = border_style(theme, focused);
    if focused {
        border = border.add_modifier(Modifier::BOLD);
    }
    let block = Block::default()
        .title("Anime")
        .borders(Borders::ALL)
        .border_style(border);

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(48),
            Constraint::Percentage(20),
            Constraint::Percentage(12),
            Constraint::Percentage(20),
        ],
    )
    .header(Row::new(vec![
        Cell::from(Span::styled("Title", theme.title_style())),
        Cell::from(Span::styled("Status", theme.title_style())),
        Cell::from(Span::styled("Score", theme.title_style())),
        Cell::from(Span::styled("Progress", theme.title_style())),
    ]))
    .block(block)
    .column_spacing(1)
    .row_highlight_style(theme.selected_item_style())
    .highlight_symbol("▶ ");
    frame.render_stateful_widget(table, area, &mut state);
}

pub(super) fn build_episode_items(app: &App) -> Vec<ListItem<'_>> {
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
