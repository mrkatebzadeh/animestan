use std::{
    fs, io,
    path::{Path, PathBuf},
};

use animestan_core::{AppConfig, CoreResult};
use anyhow::{Context, anyhow};
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

const DEFAULT_THEME_TOML: &str = r##"# Animestan TUI theme configuration
# Every panel, title, item, selected item, and non-interactive text uses
# the colors defined below. Modify any value to change the look of the UI.

[panels]
border = "#737994"
focused_border = "#8CAAEE"

[titles]
fg = "#A6D189"

[items]
fg = "#C6D0F5"

[selected_item]
fg = "#232634"
bg = "#F2D5CF"

[non_interactive]
fg = "#838BA7"

[heatmap]
watched = "#A6D189"
in_progress = "#E5C890"
upcoming = "#838BA7"
"##;

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

#[allow(dead_code)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ThemeConfig {
    panels: PanelConfig,
    titles: ColorConfig,
    items: ColorConfig,
    selected_item: ColorConfig,
    heatmap: HeatmapConfig,
    non_interactive: ColorConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PanelConfig {
    border: String,
    focused_border: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ColorConfig {
    fg: String,
    #[serde(default)]
    bg: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct HeatmapConfig {
    watched: String,
    in_progress: String,
    upcoming: String,
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

    pub fn load(config: &AppConfig) -> CoreResult<Self> {
        let path = Self::config_path(config);
        match fs::read_to_string(&path) {
            Ok(contents) => Self::from_toml(&contents),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                Self::create_default_file(&path)?;
                Self::from_toml(DEFAULT_THEME_TOML)
            }
            Err(err) => {
                Err(err).context(format!("failed to read theme file at {}", path.display()))?
            }
        }
    }

    fn config_path(_config: &AppConfig) -> PathBuf {
        AppConfig::config_dir().join("theme.toml")
    }

    fn create_default_file(path: &Path) -> CoreResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context(format!(
                "failed to create theme directory at {}",
                parent.display()
            ))?;
        }
        fs::write(path, DEFAULT_THEME_TOML).context(format!(
            "failed to write default theme file at {}",
            path.display()
        ))?;
        Ok(())
    }

    fn from_toml(contents: &str) -> CoreResult<Self> {
        let parsed: ThemeConfig = toml::from_str(contents).context("failed to parse theme.toml")?;
        Self::from_config(parsed)
    }

    fn from_config(parsed: ThemeConfig) -> CoreResult<Self> {
        Ok(Self {
            panels: PanelStyle {
                border: parse_color_string(&parsed.panels.border)?,
                focused_border: parse_color_string(&parsed.panels.focused_border)?,
            },
            titles: TextStyle::from_config(parsed.titles)?,
            items: TextStyle::from_config(parsed.items)?,
            selected_item: TextStyle::from_config(parsed.selected_item)?,
            heatmap: HeatmapPalette {
                watched: parse_rgb_string(&parsed.heatmap.watched)?,
                in_progress: parse_rgb_string(&parsed.heatmap.in_progress)?,
                upcoming: parse_rgb_string(&parsed.heatmap.upcoming)?,
            },
            non_interactive: TextStyle::from_config(parsed.non_interactive)?,
        })
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

    fn from_config(config: ColorConfig) -> CoreResult<Self> {
        Ok(Self {
            fg: parse_color_string(&config.fg)?,
            bg: config
                .bg
                .map(|value| parse_color_string(&value))
                .transpose()?,
        })
    }
}

fn parse_color_string(value: &str) -> CoreResult<Color> {
    let trimmed = value.trim();
    if let Some(rgb) = parse_hex_color(trimmed) {
        return Ok(Color::Rgb(rgb.0, rgb.1, rgb.2));
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "black" => Ok(Color::Black),
        "white" => Ok(Color::White),
        "darkgray" | "darkgrey" => Ok(Color::DarkGray),
        "gray" | "grey" => Ok(Color::Gray),
        "cyan" => Ok(Color::Cyan),
        "yellow" => Ok(Color::Yellow),
        "magenta" => Ok(Color::Magenta),
        "red" => Ok(Color::Red),
        "green" => Ok(Color::Green),
        "blue" => Ok(Color::Blue),
        other => Err(anyhow!("unknown color '{other}'")),
    }
}

fn parse_rgb_string(value: &str) -> CoreResult<(u8, u8, u8)> {
    let color = parse_color_string(value)?;
    color_to_rgb(color)
}

fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let normalized = value.trim();
    let normalized = normalized.strip_prefix('#').unwrap_or(normalized);
    if normalized.len() != 6 {
        return None;
    }

    let decoded = u32::from_str_radix(normalized, 16).ok()?;
    let red = ((decoded >> 16) & 0xFF) as u8;
    let green = ((decoded >> 8) & 0xFF) as u8;
    let blue = (decoded & 0xFF) as u8;
    Some((red, green, blue))
}

fn color_to_rgb(color: Color) -> CoreResult<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Ok((r, g, b)),
        Color::Black => Ok((0, 0, 0)),
        Color::White => Ok((255, 255, 255)),
        Color::DarkGray => Ok((169, 169, 169)),
        Color::Gray => Ok((128, 128, 128)),
        Color::Yellow => Ok((255, 255, 0)),
        Color::Cyan => Ok((0, 255, 255)),
        Color::Magenta => Ok((255, 0, 255)),
        Color::Red => Ok((255, 0, 0)),
        Color::Green => Ok((0, 128, 0)),
        Color::Blue => Ok((0, 0, 255)),
        other => Err(anyhow!("color {other:?} cannot be used for heatmap")),
    }
}
