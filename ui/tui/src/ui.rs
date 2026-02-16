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

use animestan_core::Episode;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::block::Title;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};

use crate::app::{
    App, ConfirmExitChoice, EpisodeIndicators, FilterTarget, Focus, InputMode, LeftPaneMode,
};
use crate::events::keybindings;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(7),
            Constraint::Length(4),
        ])
        .split(frame.area());

    render_hint_panel(frame, chunks[0]);
    render_search_bar(frame, chunks[1], app);

    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

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

    render_episode_heatmap(frame, chunks[3], app);
    render_details(frame, chunks[4], app);
    render_keybindings_modal(frame, app);
    render_exit_confirmation_modal(frame, app);
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem>,
    active_index: usize,
    focused: bool,
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
    let block = Block::default()
        .title("Search Anime")
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

fn render_hint_panel(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(false));
    let paragraph = Paragraph::new("Press ? to list keybinding")
        .alignment(Alignment::Center)
        .block(block);
    frame.render_widget(paragraph, area);
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
    if let Some(playing_id) = app.current_playing_episode_id() {
        let mut now_playing = String::from("Now playing: ▶");
        if app.current_episode_id().is_some_and(|id| id == playing_id) {
            if let Some(title) = app.current_episode_title() {
                now_playing.push(' ');
                now_playing.push_str(&title);
            }
        }
        lines.push(Line::from(now_playing));
    }
    let left_status = format!(
        "Mode: {} | {} | {}",
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
                '♥'
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
            ListItem::new(format!("{marker} ♥ {}", entry.anime.title))
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

fn render_episode_heatmap(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default()
        .title("Episode Heatmap")
        .borders(Borders::ALL)
        .border_style(border_style(false));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    render_heatmap_grid(frame, chunks[0], app);
    render_heatmap_info(frame, chunks[1], app);
}

fn render_heatmap_grid(frame: &mut Frame, area: Rect, app: &App) {
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

            if !spans.is_empty() {
                spans.push(Span::raw(" "));
            }

            let indicators = app.episode_indicators(&episodes[idx].id);
            let mut style = heatmap_cell_style(indicators, normalized[idx]);
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

fn render_heatmap_info(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let lines = if let Some(episode) = app.current_episode() {
        let mut info = vec![Line::from(format!(
            "#{:03} — {}",
            episode.number, episode.title
        ))];

        if let Some(duration) = episode.duration_secs {
            info.push(Line::from(format!(
                "Duration: {}",
                format_duration(duration)
            )));
        }

        if let Some(synopsis) = episode.synopsis.as_deref() {
            let trimmed = synopsis.trim();
            if !trimmed.is_empty() {
                info.push(Line::from(trimmed));
            }
        }

        info
    } else {
        vec![Line::from("Select an episode to show title and synopsis.")]
    };

    let info = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    frame.render_widget(info, area);
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
    if width == 0 {
        return 0;
    }
    ((width + 1).saturating_div(2)).max(1)
}

fn heatmap_cell_style(indicators: EpisodeIndicators, intensity: f64) -> Style {
    let base = if indicators.watched {
        (32, 180, 90)
    } else if indicators.in_progress {
        (225, 200, 70)
    } else {
        (110, 115, 140)
    };

    let color = tinted_color(base, intensity);
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

fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
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

fn render_exit_confirmation_modal(frame: &mut Frame, app: &App) {
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
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
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
        Line::from(Span::styled(
            QUESTION_TEXT,
            Style::default().fg(Color::White),
        )),
        Line::default(),
        button_line,
        Line::default(),
        Line::from(Span::styled(
            HINT_TEXT,
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let content_height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let height = content_height.saturating_add(2).min(frame_area.height);

    let area = centered_rect(frame_area, width, height);
    let block = Block::default().title("Confirm Exit").borders(Borders::ALL);
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(block)
        .wrap(Wrap { trim: true });

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
