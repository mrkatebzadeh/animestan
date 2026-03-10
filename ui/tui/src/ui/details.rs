use animestan_core::{
    AnimeMetadata, format_list, format_season_year, format_status_score, metadata_source_label,
};
use ratatui::layout::Rect;
use ratatui::prelude::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::theme::Theme;

use super::border_style;

pub(super) fn render_anime_details_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default()
        .title("Details")
        .borders(Borders::ALL)
        .border_style(border_style(theme, app.info_modal_visible()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let lines = build_details_panel_lines(app, theme);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

pub(super) fn build_info_modal_lines<'a>(app: &'a App, theme: &Theme) -> Vec<Line<'a>> {
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

fn build_details_panel_lines<'a>(app: &'a App, theme: &Theme) -> Vec<Line<'a>> {
    let mut lines = metadata_section_lines(
        app.info_modal_metadata(),
        app.info_modal_error(),
        app.info_modal_loading(),
        theme,
    );
    if !app.info_modal_loading() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Press i to refresh metadata.",
            theme.non_interactive_style(),
        )));
    }
    lines
}

pub(super) fn metadata_section_lines<'a>(
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
