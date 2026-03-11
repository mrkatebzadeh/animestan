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

use crate::app::{App, ConfirmExitChoice, InputMode};
use crate::events::keybindings;
use crate::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::Frame;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};

use super::border_style;
use super::details::{build_info_modal_lines, metadata_section_lines};

const KEYBINDINGS_HEADER: [&str; 6] = [
    ".█████╗ ███╗   ██╗██╗███╗   ███╗███████╗███████╗████████╗ █████╗ ███╗   ██╗",
    "██╔══██╗████╗  ██║██║████╗ ████║██╔════╝██╔════╝╚══██╔══╝██╔══██╗████╗  ██║",
    "███████║██╔██╗ ██║██║██╔████╔██║█████╗  ███████╗   ██║   ███████║██╔██╗ ██║",
    "██╔══██║██║╚██╗██║██║██║╚██╔╝██║██╔══╝  ╚════██║   ██║   ██╔══██║██║╚██╗██║",
    "██║  ██║██║ ╚████║██║██║ ╚═╝ ██║███████╗███████║   ██║   ██║  ██║██║ ╚████║",
    "╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝╚═╝     ╚═╝╚══════╝╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═══╝",
];
const HEADER_MARGIN: &str = "  ";
const KEY_HINT_MARGIN: &str = "        ";

pub(super) fn render_keybindings_modal(frame: &mut Frame, app: &mut App, theme: &Theme) {
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

pub(super) fn render_info_modal(frame: &mut Frame, app: &App, theme: &Theme) {
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

pub(super) fn render_search_results_modal(frame: &mut Frame, app: &App, theme: &Theme) {
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

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    let input_block = Block::default()
        .title("Search Anime")
        .borders(Borders::ALL)
        .border_style(border_style(theme, app.input_mode() == InputMode::Search));
    let prompt = Line::from(vec![
        Span::styled("> ", theme.non_interactive_style()),
        Span::raw(app.search_query()),
    ]);
    let input_inner = input_block.inner(layout[0]);
    let input_paragraph = Paragraph::new(prompt).block(input_block);
    frame.render_widget(input_paragraph, layout[0]);
    if app.input_mode() == InputMode::Search {
        let typed_chars = app.search_query().chars().count();
        let typed_offset = u16::try_from(typed_chars).unwrap_or(u16::MAX);
        let cursor_base = input_inner.x.saturating_add(2);
        let max_cursor = input_inner
            .x
            .saturating_add(input_inner.width.saturating_sub(1));
        let cursor_x = cursor_base.saturating_add(typed_offset).min(max_cursor);
        frame.set_cursor_position((cursor_x, input_inner.y));
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(layout[1]);

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
        Span::styled("Enter", theme.title_style()),
        Span::raw(" to search (press again to add) · "),
        Span::styled("Ctrl+M", theme.title_style()),
        Span::raw(" to mark selection"),
    ]))
    .style(theme.non_interactive_style());
    frame.render_widget(hint, layout[2]);
}

pub(super) fn render_exit_confirmation_modal(frame: &mut Frame, app: &App, theme: &Theme) {
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

pub(super) fn render_quick_launch_palette(frame: &mut Frame, app: &App, theme: &Theme) {
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
