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

use animestan_core::{AnimeMetadata, Episode, MetadataSource};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::block::Title;
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
    TableState, Wrap,
};

use crate::app::{App, ConfirmExitChoice, EpisodeIndicators, FilterTarget, Focus, InputMode};
use crate::events::keybindings;
use crate::theme::{HeatmapVariant, Theme};

const KEYBINDINGS_HEADER: [&str; 6] = [
    "█████╗ ███╗   ██╗██╗███╗   ███╗███████╗███████╗████████╗ █████╗ ███╗   ██╗",
    "██╔══██╗████╗  ██║██║████╗ ████║██╔════╝██╔════╝╚══██╔══╝██╔══██╗████╗  ██║",
    "███████║██╔██╗ ██║██║██╔████╔██║█████╗  ███████╗   ██║   ███████║██╔██╗ ██║",
    "██╔══██║██║╚██╗██║██║██║╚██╔╝██║██╔══╝  ╚════██║   ██║   ██╔══██║██║╚██╗██║",
    "██║  ██║██║ ╚████║██║██║ ╚═╝ ██║███████╗███████║   ██║   ██║  ██║██║ ╚████║",
    "╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝╚═╝     ╚═╝╚══════╝╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═══╝",
];
const HEADER_MARGIN: &str = "  ";
const KEY_HINT_MARGIN: &str = "        ";

pub fn render(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let frame_area = frame.area();
    let heatmap_width = frame_area.width.saturating_sub(2).max(1);
    let columns = heatmap_columns(heatmap_width as usize);
    let total_episodes = app.episodes().len();
    let rows = if total_episodes == 0 {
        1
    } else {
        total_episodes.div_ceil(columns.max(1))
    };
    let total_height = frame_area.height as usize;
    let top_height = 3 + 3;
    let session_default = 7;
    let session_min = 3;
    let min_heatmap_height = 7;
    let requested_heatmap_height = rows.max(min_heatmap_height);
    let available_for_session =
        total_height.saturating_sub(top_height + requested_heatmap_height + session_min);
    let session_height_usize = available_for_session.min(session_default);
    let max_heatmap_height = total_height
        .saturating_sub(top_height + session_height_usize)
        .max(1);
    let heatmap_height = requested_heatmap_height.min(max_heatmap_height);
    let heatmap_length = u16::try_from(heatmap_height.min(u16::MAX as usize)).unwrap_or(u16::MAX);
    let list_height =
        total_height.saturating_sub(top_height + session_height_usize + heatmap_height);
    let list_length = u16::try_from(list_height.min(u16::MAX as usize)).unwrap_or(u16::MAX);
    let session_length =
        u16::try_from(session_height_usize.min(u16::MAX as usize)).unwrap_or(u16::MAX);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(list_length),
            Constraint::Length(heatmap_length),
            Constraint::Length(session_length),
        ])
        .split(frame_area);

    render_search_bar(frame, chunks[0], app, theme);

    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let left_target = FilterTarget::Anime;
    let left_filter_visible = should_show_panel_filter(app, left_target);
    let (left_filter_area, left_list_area) = split_filter_area(lists[0], left_filter_visible);
    if let Some(area) = left_filter_area {
        render_panel_filter_input(frame, area, app, left_filter_visible, theme);
    }
    render_anime_table(frame, left_list_area, app, theme);

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

    render_episode_heatmap(frame, chunks[2], app, theme);
    render_session_panel(frame, chunks[3], app, theme);
    render_search_results_modal(frame, app, theme);
    render_keybindings_modal(frame, app, theme);
    render_info_modal(frame, app, theme);
    render_exit_confirmation_modal(frame, app, theme);
    render_quick_launch_palette(frame, app, theme);
}

fn render_list(
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

fn render_anime_table(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
            let stats = app.anime_progress_for(&entry.anime.id);
            let progress = stats.map_or_else(
                || "--/--".to_string(),
                |stats| format!("{}/{}", stats.watched, stats.total),
            );
            Row::new(vec![
                Cell::from(entry.anime.title.clone()),
                Cell::from(progress),
            ])
        })
        .collect();

    let mut state = TableState::default();
    if !rows.is_empty() {
        state.select(Some(app.left_index().min(rows.len() - 1)));
    }

    let block = Block::default()
        .title("Anime")
        .borders(Borders::ALL)
        .border_style(border_style(theme, app.focus() == Focus::Left));

    let table = Table::new(
        rows,
        [Constraint::Percentage(60), Constraint::Percentage(40)],
    )
    .header(Row::new(vec![
        Cell::from(Span::styled("Title", theme.title_style())),
        Cell::from(Span::styled("Progress", theme.title_style())),
    ]))
    .block(block)
    .column_spacing(1)
    .highlight_style(theme.selected_item_style())
    .highlight_symbol("▶ ");
    frame.render_stateful_widget(table, area, &mut state);
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

fn render_panel_filter_input(
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

fn should_show_panel_filter(app: &App, target: FilterTarget) -> bool {
    app.panel_filter_mode() && app.panel_filter_target() == Some(target)
}

fn render_search_bar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::default()
        .title("Search Anime")
        .borders(Borders::ALL)
        .border_style(border_style(theme, app.input_mode() == InputMode::Search));

    let prompt = Line::from(vec![
        Span::styled("> ", theme.non_interactive_style()),
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

fn render_session_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
        app.current_selection_label()
    );
    let hint_text = "Press ? for keybindings";
    let total_width = chunks[2].width as usize;
    let status_len = left_status.chars().count();
    let hint_len = hint_text.chars().count();
    let spacing = total_width.saturating_sub(status_len + hint_len);
    let spacer = " ".repeat(spacing);

    let status_line = Line::from(vec![
        Span::styled(
            left_status,
            theme.item_style().bg(theme.non_interactive_color()),
        ),
        Span::raw(spacer),
        Span::styled(
            hint_text,
            Style::default()
                .bg(theme.non_interactive_color())
                .add_modifier(Modifier::REVERSED),
        ),
    ]);
    let status =
        Paragraph::new(status_line).style(Style::default().bg(theme.non_interactive_color()));
    frame.render_widget(status, chunks[2]);
}

fn format_elapsed(seconds: Option<f64>) -> String {
    let secs = seconds.unwrap_or(0.0).max(0.0);
    let duration = std::time::Duration::from_secs_f64(secs);
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
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

fn render_episode_heatmap(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default()
        .title("Progress")
        .borders(Borders::ALL)
        .border_style(border_style(theme, false));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    render_heatmap_grid(frame, inner, app, theme);
}

fn render_heatmap_grid(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let episodes = app.episodes();
    if episodes.is_empty() {
        let placeholder =
            Paragraph::new("Load episodes to view heatmap").alignment(Alignment::Center);
        frame.render_widget(placeholder, area);
        return;
    }

    let normalized = heatmap_scalars(episodes);
    let columns = heatmap_columns(area.width as usize);
    if columns == 0 {
        return;
    }

    let selected = app.current_episode_index();
    let rows = episodes.len().div_ceil(columns);
    let mut lines = Vec::new();

    for row in 0..rows {
        let start = row * columns;
        let mut spans = Vec::new();
        for col in 0..columns {
            let idx = start + col;
            if idx >= episodes.len() {
                break;
            }

            let alternate = (row + col) % 2 == 0;

            let indicators = app.episode_indicators(&episodes[idx].id);
            let mut style = heatmap_cell_style(indicators, normalized[idx], theme, alternate);
            if selected == Some(idx) {
                style = style.add_modifier(Modifier::REVERSED);
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            spans.push(Span::styled("▉", style));
        }

        if spans.is_empty() {
            continue;
        }

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        let placeholder = Paragraph::new("No episodes available").alignment(Alignment::Center);
        frame.render_widget(placeholder, area);
        return;
    }

    let grid = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    frame.render_widget(grid, area);
}

#[allow(clippy::cast_precision_loss)]
fn heatmap_scalars(episodes: &[Episode]) -> Vec<f64> {
    if episodes.is_empty() {
        return Vec::new();
    }

    let has_air_dates = episodes.iter().any(|episode| episode.air_date.is_some());
    let offset = if has_air_dates {
        episodes
            .iter()
            .filter_map(|episode| episode.air_date)
            .min()
            .unwrap_or(0)
    } else {
        0
    };

    let values: Vec<f64> = episodes
        .iter()
        .map(|episode| {
            let raw = if has_air_dates {
                episode
                    .air_date
                    .unwrap_or(offset + i64::from(episode.number))
            } else {
                i64::from(episode.number)
            };
            raw as f64
        })
        .collect();

    let min_value = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_value = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = max_value - min_value;

    if range.abs() < f64::EPSILON {
        return vec![0.5; values.len()];
    }

    values
        .into_iter()
        .map(|value| ((value - min_value) / range).clamp(0.0, 1.0))
        .collect()
}

fn heatmap_columns(width: usize) -> usize {
    width.max(1)
}

fn heatmap_cell_style(
    indicators: EpisodeIndicators,
    intensity: f64,
    theme: &Theme,
    alternate: bool,
) -> Style {
    let color = if indicators.watched {
        let base = theme.heatmap_color(HeatmapVariant::Watched);
        let adjusted = (intensity + if alternate { 0.15 } else { 0.0 }).clamp(0.0, 1.0);
        tinted_color(base, adjusted)
    } else if alternate {
        Color::Rgb(80, 80, 80)
    } else {
        Color::Rgb(48, 48, 48)
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn tinted_color((r, g, b): (u8, u8, u8), intensity: f64) -> Color {
    let factor = 0.5 + intensity.clamp(0.0, 1.0) * 0.5;
    let red = (f64::from(r) * factor).clamp(0.0, 255.0) as u8;
    let green = (f64::from(g) * factor).clamp(0.0, 255.0) as u8;
    let blue = (f64::from(b) * factor).clamp(0.0, 255.0) as u8;
    Color::Rgb(red, green, blue)
}

fn border_style(theme: &Theme, focused: bool) -> Style {
    theme.panel_border_style(focused)
}

fn render_keybindings_modal(frame: &mut Frame, app: &mut App, theme: &Theme) {
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
    for header in KEYBINDINGS_HEADER {
        lines.push(Line::from(format!(
            "{HEADER_MARGIN}{header}{HEADER_MARGIN}"
        )));
    }
    lines.push(Line::default());
    let mut current_mode: Option<InputMode> = None;
    for binding in bindings {
        if current_mode != Some(binding.mode) {
            if current_mode.is_some() {
                lines.push(Line::default());
            }
            current_mode = Some(binding.mode);
            lines.push(Line::from(Span::styled(
                format!("{} mode", input_mode_label(binding.mode)),
                theme.title_style(),
            )));
        }

        let key_label = format!("{:<width$}", binding.keys, width = key_width);
        lines.push(Line::from(vec![
            Span::styled(key_label, Style::default().fg(theme.title_color())),
            Span::raw(KEY_HINT_MARGIN),
            Span::raw(binding.description),
        ]));
    }

    app.set_keybindings_content_lines(lines.len());
    let frame_area = frame.area();
    let mut width = frame_area.width.min(80);
    let min_width = 40u16;
    width = width.max(min_width).min(frame_area.width);
    let computed_height = u32::from(frame_area.height).saturating_mul(70) / 100;
    let mut height = u16::try_from(computed_height).unwrap_or(u16::MAX);
    let min_height = 10u16;
    if height < min_height {
        height = min_height;
    }
    height = height.min(frame_area.height);
    if width == 0 || height == 0 {
        return;
    }

    let area = centered_rect(frame_area, width, height);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(theme, false));
    let inner = block.inner(area);
    app.set_keybindings_viewport_lines(inner.height as usize);
    let scroll_offset = u16::try_from(app.keybindings_scroll()).unwrap_or(u16::MAX);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll_offset, 0));

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn render_info_modal(frame: &mut Frame, app: &App, theme: &Theme) {
    if !app.info_modal_visible() {
        return;
    }

    let frame_area = frame.area();
    if frame_area.width < 20 || frame_area.height < 10 {
        return;
    }

    let width = frame_area.width.saturating_sub(6).min(110);
    let height = frame_area.height.saturating_sub(6);
    if width == 0 || height == 0 {
        return;
    }

    let area = centered_rect(frame_area, width, height);
    let title = app
        .info_modal_metadata()
        .map(|metadata| metadata.title.clone())
        .or_else(|| app.current_anime_title())
        .unwrap_or_else(|| "Anime Info".to_string());

    let block = Block::default()
        .title(Span::styled(title, theme.title_style()))
        .borders(Borders::ALL)
        .border_style(border_style(theme, false));

    let paragraph = Paragraph::new(build_info_modal_lines(app, theme))
        .wrap(Wrap { trim: true })
        .block(block);

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn render_search_results_modal(frame: &mut Frame, app: &App, theme: &Theme) {
    if !app.search_results_modal_visible() {
        return;
    }

    let frame_area = frame.area();
    if frame_area.width < 40 || frame_area.height < 10 {
        return;
    }

    let width = frame_area.width.saturating_sub(6).min(120);
    let height = frame_area.height.saturating_sub(6);
    if width == 0 || height == 0 {
        return;
    }

    let title = format!("Search: {}", app.search_results_query());
    let block = Block::default()
        .title(Span::styled(title, theme.title_style()))
        .borders(Borders::ALL)
        .border_style(border_style(theme, true));
    let area = centered_rect(frame_area, width, height);

    frame.render_widget(Clear, area);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);

    let results = app.search_results();
    let items: Vec<ListItem> = if results.is_empty() {
        vec![ListItem::new("No results")]
    } else {
        results
            .iter()
            .map(|entry| ListItem::new(entry.title.clone()))
            .collect()
    };

    let mut state = ListState::default();
    if !results.is_empty() {
        state.select(Some(app.search_results_selection()));
    }

    let list_block = Block::default()
        .title("Matches")
        .borders(Borders::ALL)
        .border_style(border_style(theme, true));
    let list = List::new(items)
        .block(list_block)
        .highlight_style(theme.selected_item_style())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, columns[0], &mut state);

    let metadata_lines = metadata_section_lines(
        app.search_results_metadata(),
        app.search_results_metadata_error(),
        app.search_results_metadata_loading(),
        theme,
    );
    let metadata_block = Block::default()
        .title("Info")
        .borders(Borders::ALL)
        .border_style(border_style(theme, true));
    let metadata = Paragraph::new(metadata_lines)
        .block(metadata_block)
        .wrap(Wrap { trim: true });
    frame.render_widget(metadata, columns[1]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Esc", theme.title_style()),
        Span::raw(" to close · "),
        Span::styled("Ctrl+M", theme.title_style()),
        Span::raw(" to mark selection"),
    ]))
    .style(theme.non_interactive_style());
    frame.render_widget(hint, chunks[1]);
}

fn metadata_section_lines<'a>(
    metadata: Option<&'a AnimeMetadata>,
    error: Option<&'a str>,
    loading: bool,
    theme: &Theme,
) -> Vec<Line<'a>> {
    if loading {
        return vec![
            Line::from(Span::styled(
                "Loading anime metadata...",
                theme.title_style(),
            )),
            Line::default(),
            Line::from("This may take a moment. Press Esc to cancel."),
        ];
    }

    if let Some(error) = error {
        return vec![
            Line::from(Span::styled(
                "Failed to load metadata:",
                theme.selected_item_style(),
            )),
            Line::from(error),
            Line::default(),
        ];
    }

    if let Some(metadata) = metadata {
        let mut lines = Vec::new();
        let status_score = format_status_score(metadata.status.as_deref(), metadata.score);
        let season_year = format_season_year(metadata.season.as_deref(), metadata.year);
        let genres = format_list(&metadata.genres);
        let studios = format_list(&metadata.studios);
        let synopsis = metadata
            .synopsis
            .as_deref()
            .map(str::trim)
            .filter(|text: &&str| !text.is_empty())
            .unwrap_or("Synopsis not available.");
        lines.push(Line::from(Span::styled(
            format!("Status / Score: {status_score}"),
            theme.title_style(),
        )));
        lines.push(Line::from(format!("Season / Year: {season_year}")));
        lines.push(Line::from(format!("Genres: {genres}")));
        lines.push(Line::from(format!("Studios: {studios}")));
        lines.push(Line::default());
        lines.push(Line::from("Synopsis:"));
        lines.push(Line::from(synopsis));
        lines.push(Line::default());
        lines.push(Line::from(format!(
            "Trailer: {}",
            metadata.trailer_url.as_deref().unwrap_or("N/A")
        )));
        lines.push(Line::from(format!(
            "Source: {} ({})",
            &metadata.source_url,
            metadata_source_label(metadata.source)
        )));
        lines.push(Line::default());
        return lines;
    }

    vec![Line::from("No metadata available for selection.")]
}

fn build_info_modal_lines<'a>(app: &'a App, theme: &Theme) -> Vec<Line<'a>> {
    let mut lines = metadata_section_lines(
        app.info_modal_metadata(),
        app.info_modal_error(),
        app.info_modal_loading(),
        theme,
    );
    if !app.info_modal_loading() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Press Esc to close.",
            theme.non_interactive_style(),
        )));
    }
    lines
}

fn format_status_score(status: Option<&str>, score: Option<f32>) -> String {
    match (status, score) {
        (Some(status), Some(score)) => format!("{status} / {score:.1}"),
        (Some(status), None) => status.to_string(),
        (None, Some(score)) => format!("Score {score:.1}"),
        _ => "N/A".to_string(),
    }
}

fn format_list(items: &[String]) -> String {
    if items.is_empty() {
        "N/A".to_string()
    } else {
        items.join(", ")
    }
}

fn format_season_year(season: Option<&str>, year: Option<u16>) -> String {
    match (season, year) {
        (Some(season), Some(year)) => format!("{season} {year}"),
        (Some(season), None) => season.to_string(),
        (None, Some(year)) => year.to_string(),
        _ => "N/A".to_string(),
    }
}

fn metadata_source_label(source: MetadataSource) -> &'static str {
    match source {
        MetadataSource::AniList => "AniList",
        MetadataSource::Kitsu => "Kitsu",
    }
}

fn render_exit_confirmation_modal(frame: &mut Frame, app: &App, theme: &Theme) {
    const QUESTION_TEXT: &str = "Exit Animestan?";
    const BUTTON_ROW_TEXT: &str = "[ Yes ]   [ No ]";
    const HINT_TEXT: &str = "Use ←/→/Tab to switch, Enter to confirm.";

    if !app.confirm_exit() {
        return;
    }

    let frame_area = frame.area();
    if frame_area.width < 20 || frame_area.height < 5 {
        return;
    }

    let question_width = u16::try_from(QUESTION_TEXT.len()).unwrap_or(u16::MAX);
    let button_width = u16::try_from(BUTTON_ROW_TEXT.len()).unwrap_or(u16::MAX);
    let hint_width = u16::try_from(HINT_TEXT.len()).unwrap_or(u16::MAX);
    let max_content_width = question_width.max(button_width).max(hint_width);
    let desired_width = max_content_width.saturating_add(8);
    let width = desired_width.min(frame_area.width);

    let yes_selected = matches!(app.confirm_exit_choice(), ConfirmExitChoice::Yes);
    let button_style = |selected| {
        if selected {
            theme.selected_item_style()
        } else {
            theme.item_style()
        }
    };

    let button_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("[ Yes ]", button_style(yes_selected)),
        Span::raw("   "),
        Span::styled("[ No ]", button_style(!yes_selected)),
        Span::raw(" "),
    ]);

    let lines = vec![
        Line::from(Span::styled(QUESTION_TEXT, theme.title_style())),
        Line::default(),
        button_line,
        Line::default(),
        Line::from(Span::styled(HINT_TEXT, theme.non_interactive_style())),
    ];

    let content_height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let height = content_height.saturating_add(2).min(frame_area.height);

    let area = centered_rect(frame_area, width, height);
    let block = Block::default()
        .title("Confirm Exit")
        .borders(Borders::ALL)
        .border_style(border_style(theme, true));
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn render_quick_launch_palette(frame: &mut Frame, app: &App, theme: &Theme) {
    if !app.quick_launch_active() {
        return;
    }

    let candidates = app.quick_launch_items();
    let list_height = u16::try_from(candidates.len()).unwrap_or(u16::MAX);
    let height = (list_height + 6).max(8);
    let frame_area = frame.area();
    if frame_area.width < 30 || frame_area.height < height {
        return;
    }

    let width = frame_area.width.saturating_sub(8).min(80);
    let area = centered_rect(frame_area, width, height);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title("Quick Launch")
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(border_style(theme, true));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let prompt = Line::from(vec![
        Span::styled("> ", theme.non_interactive_style()),
        Span::raw(app.quick_launch_query()),
    ]);
    let paragraph = Paragraph::new(prompt);
    frame.render_widget(paragraph, chunks[0]);
    let typed_chars = app.quick_launch_query().chars().count();
    let typed_offset = u16::try_from(typed_chars).unwrap_or(u16::MAX);
    let cursor_base = chunks[0].x.saturating_add(2);
    let max_cursor = chunks[0]
        .x
        .saturating_add(chunks[0].width.saturating_sub(1));
    let cursor_x = cursor_base.saturating_add(typed_offset).min(max_cursor);
    frame.set_cursor_position((cursor_x, chunks[0].y));

    let items = if candidates.is_empty() {
        vec![ListItem::new("No quick launch items")]
    } else {
        candidates
            .iter()
            .map(|candidate| ListItem::new(candidate.label.clone()))
            .collect()
    };

    let mut state = ListState::default();
    if !items.is_empty() {
        let idx = app.quick_launch_selection().min(items.len() - 1);
        state.select(Some(idx));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(theme.selected_item_style())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, chunks[1], &mut state);

    let hint = Paragraph::new(Line::from(vec![
        Span::raw("Enter to run · Esc to close"),
        Span::styled(
            " · Ctrl+K opens this palette",
            theme.non_interactive_style(),
        ),
    ]))
    .style(theme.non_interactive_style());
    frame.render_widget(hint, chunks[2]);
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
