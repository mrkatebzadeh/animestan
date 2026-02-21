use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub struct Theme {
    panels: PanelStyle,
    titles: TextStyle,
    items: TextStyle,
    selected_item: TextStyle,
    heatmap: HeatmapPalette,
    non_interactive: TextStyle,
}

#[derive(Clone, Debug)]
struct PanelStyle {
    border: Color,
    focused_border: Color,
}

#[derive(Clone, Copy, Debug)]
pub enum HeatmapVariant {
    Watched,
    InProgress,
    Upcoming,
}

#[derive(Clone, Debug)]
struct HeatmapPalette {
    watched: (u8, u8, u8),
    in_progress: (u8, u8, u8),
    upcoming: (u8, u8, u8),
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
            heatmap: HeatmapPalette {
                watched: (32, 180, 90),
                in_progress: (225, 200, 70),
                upcoming: (110, 115, 140),
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

    #[must_use]
    pub fn title_style(&self) -> Style {
        self.titles.style().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn item_style(&self) -> Style {
        self.items.style()
    }

    #[must_use]
    pub fn non_interactive_color(&self) -> Color {
        self.non_interactive.fg
    }

    #[must_use]
    pub fn title_color(&self) -> Color {
        self.titles.fg
    }

    #[must_use]
    pub fn heatmap_color(&self, variant: HeatmapVariant) -> (u8, u8, u8) {
        match variant {
            HeatmapVariant::Watched => self.heatmap.watched,
            HeatmapVariant::InProgress => self.heatmap.in_progress,
            HeatmapVariant::Upcoming => self.heatmap.upcoming,
        }
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
