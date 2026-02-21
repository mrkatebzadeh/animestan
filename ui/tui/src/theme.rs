use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub struct Theme {
    panels: PanelStyle,
    titles: TextStyle,
    items: TextStyle,
    selected_item: TextStyle,
    non_interactive: TextStyle,
}

#[derive(Clone, Debug)]
struct PanelStyle {
    border: Color,
    focused_border: Color,
}

#[derive(Clone, Debug)]
struct TextStyle {
    fg: Color,
    bg: Option<Color>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            panels: PanelStyle {
                border: Color::Gray,
                focused_border: Color::Cyan,
            },
            titles: TextStyle {
                fg: Color::Cyan,
                bg: None,
            },
            items: TextStyle {
                fg: Color::White,
                bg: None,
            },
            selected_item: TextStyle {
                fg: Color::Yellow,
                bg: None,
            },
            non_interactive: TextStyle {
                fg: Color::DarkGray,
                bg: None,
            },
        }
    }
}

impl Theme {
    #[must_use]
    pub fn panel_border_style(&self, focused: bool) -> Style {
        let color = if focused {
            self.panels.focused_border
        } else {
            self.panels.border
        };
        Style::default().fg(color)
    }

    #[must_use]
    pub fn selected_item_style(&self) -> Style {
        self.selected_item.style().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn non_interactive_style(&self) -> Style {
        self.non_interactive.style()
    }
}

impl TextStyle {
    #[must_use]
    fn style(&self) -> Style {
        let mut style = Style::default().fg(self.fg);
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        style
    }
}
