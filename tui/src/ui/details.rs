use animestan_core::{AnimeMetadata, format_list, format_season_year, format_status_score};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::theme::Theme;

use super::preview::PreviewWidget;

pub(super) fn render_anime_details_panel(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    theme: &Theme,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default()
        .title("Details")
        .borders(Borders::ALL)
        .border_style(theme.panel_border_style(app.info_modal_visible()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let cover_width = 14u16;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(cover_width), Constraint::Min(0)])
        .split(inner);

    let synopsis_area = columns[1];
    let mut lines = build_details_panel_lines(app, theme);
    let max_lines = synopsis_area.height as usize;
    if max_lines > 0 && lines.len() > max_lines {
        lines.truncate(max_lines);
        lines[max_lines - 1] = Line::from("...");
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, synopsis_area);

    let (image_id, can_display_images) = {
        let metadata = app
            .info_modal_metadata()
            .or_else(|| app.cached_metadata_for_current_anime());
        let image_id = if metadata.and_then(|data| data.image_url.as_ref()).is_some() {
            app.current_anime_id().unwrap_or_default()
        } else {
            String::new()
        };
        (image_id, app.can_display_images())
    };

    let preview = PreviewWidget {
        id: image_id.as_str(),
        title: "",
        can_display_images,
        theme,
    };
    frame.render_stateful_widget(preview, columns[0], app.image_state_mut());
}

pub(super) fn build_info_modal_lines<'a>(app: &'a App, theme: &Theme) -> Vec<Line<'a>> {
    let metadata = app
        .info_modal_metadata()
        .or_else(|| app.cached_metadata_for_current_anime());
    let mut lines = metadata_section_lines(
        metadata,
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

fn build_details_panel_lines<'a>(app: &'a App, theme: &Theme) -> Vec<Line<'a>> {
    let metadata = app
        .info_modal_metadata()
        .or_else(|| app.cached_metadata_for_current_anime());
    if let Some(metadata) = metadata {
        let synopsis = metadata
            .synopsis
            .as_deref()
            .map(str::trim)
            .filter(|text: &&str| !text.is_empty())
            .unwrap_or("Synopsis not available.");
        return vec![Line::from(synopsis)];
    }

    if let Some(error) = app.info_modal_error() {
        return vec![
            Line::from(Span::styled(
                "Failed to load metadata:",
                theme.selected_item_style(),
            )),
            Line::from(error),
        ];
    }

    if app.info_modal_loading() {
        return vec![
            Line::from(Span::styled(
                "Loading anime metadata...",
                theme.title_style(),
            )),
            Line::from("This may take a moment."),
        ];
    }

    vec![Line::from("Synopsis not available.")]
}

pub(super) fn metadata_section_lines<'a>(
    metadata: Option<&'a AnimeMetadata>,
    error: Option<&'a str>,
    loading: bool,
    theme: &Theme,
) -> Vec<Line<'a>> {
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
            "Source: {} (AniDB)",
            metadata.source_url,
        )));
        lines.push(Line::default());
        return lines;
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

    vec![Line::from("No metadata available for selection.")]
}
