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

use crate::app::{App, EpisodeIndicators};
use crate::theme::{HeatmapVariant, Theme};
use animestan_core::Episode;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::border_style;

pub(super) fn render_episode_heatmap(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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

pub(super) fn render_heatmap_grid(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
pub(super) fn heatmap_scalars(episodes: &[Episode]) -> Vec<f64> {
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

pub(super) fn heatmap_columns(width: usize) -> usize {
    width.max(1)
}

pub(super) fn heatmap_cell_style(
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
