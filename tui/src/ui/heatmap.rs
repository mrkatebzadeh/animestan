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
use crate::theme::{HeatmapVariant, Theme};
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Frame;
use ratatui::style::{Color, Style};
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

    let episodes = app.episodes();
    if episodes.is_empty() {
        let placeholder =
            Paragraph::new("Load episodes to view progress").alignment(Alignment::Center);
        frame.render_widget(placeholder, inner);
        return;
    }

    let total = episodes.len();
    let watched = episodes
        .iter()
        .filter(|episode| app.episode_indicators(&episode.id).watched)
        .count();
    #[allow(clippy::cast_precision_loss)]
    let percent = if total == 0 {
        0.0
    } else {
        (watched as f64 * 100.0) / total as f64
    };

    let percent_text = format!(" {watched}/{total} ({percent:.0}%)");
    let inner_width = inner.width as usize;
    if inner_width == 0 {
        return;
    }

    let text_width = percent_text.chars().count();
    if text_width >= inner_width {
        let truncated: String = percent_text.chars().take(inner_width).collect();
        let progress = Paragraph::new(Line::from(Span::styled(truncated, theme.title_style())))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(progress, inner);
        return;
    }

    let bar_width = inner_width - text_width;
    if bar_width == 0 {
        let progress = Paragraph::new(Line::from(Span::styled(
            percent_text.trim(),
            theme.title_style(),
        )))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        frame.render_widget(progress, inner);
        return;
    }

    let fill_segments = if total == 0 {
        0
    } else {
        (watched * bar_width + total / 2) / total
    };
    let fill_segments = fill_segments.min(bar_width);
    let empty_segments = bar_width - fill_segments;

    let fill_str = if fill_segments == 0 {
        String::new()
    } else {
        "█".repeat(fill_segments)
    };
    let empty_str = if empty_segments == 0 {
        String::new()
    } else {
        "░".repeat(empty_segments)
    };

    let mut spans = Vec::new();
    if !fill_str.is_empty() {
        let color = theme.heatmap_color(HeatmapVariant::Watched);
        let fill_style = Style::default().fg(Color::Rgb(color.0, color.1, color.2));
        spans.push(Span::styled(fill_str, fill_style));
    }
    if !empty_str.is_empty() {
        let empty_color = theme.non_interactive_color();
        let empty_style = Style::default().fg(empty_color);
        spans.push(Span::styled(empty_str, empty_style));
    }
    spans.push(Span::styled(percent_text, theme.title_style()));

    let paragraph = Paragraph::new(Line::from(spans))
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}
