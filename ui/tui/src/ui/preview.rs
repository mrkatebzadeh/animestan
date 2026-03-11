use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, StatefulWidget, Widget, Wrap};
use ratatui_image::Image;
use ratatui_image::protocol::Protocol;
use throbber_widgets_tui::Throbber;

use crate::app::ImageState;
use crate::theme::Theme;

use super::border_style;

pub struct PreviewWidget<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub can_display_images: bool,
    pub theme: &'a Theme,
}

impl StatefulWidget for PreviewWidget<'_> {
    type State = ImageState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut ImageState) {
        let block = Block::default()
            .title(self.title)
            .borders(Borders::ALL)
            .border_style(border_style(self.theme, false));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        state.set_area(inner);

        if !self.can_display_images {
            render_placeholder(buf, inner, "Image preview not supported", self.theme);
            return;
        }

        if self.id.is_empty() {
            render_placeholder(buf, inner, "Cover not available", self.theme);
            return;
        }

        if let Some(protocol) = state.get_image_state(self.id) {
            render_image(buf, inner, protocol);
            return;
        }

        let throbber =
            Throbber::default().label(Span::styled("Loading cover…", self.theme.title_style()));
        let loader_state = state.throbber_mut();
        StatefulWidget::render(throbber, inner, buf, loader_state);
    }
}

fn render_image(buf: &mut Buffer, area: Rect, protocol: &Protocol) {
    Image::new(protocol).render(area, buf);
}

fn render_placeholder(buf: &mut Buffer, area: Rect, text: &str, theme: &Theme) {
    let paragraph = Paragraph::new(Line::from(vec![Span::styled(
        text,
        theme.non_interactive_style(),
    )]))
    .wrap(Wrap { trim: true })
    .alignment(Alignment::Center)
    .style(Style::default());
    paragraph.render(area, buf);
}
