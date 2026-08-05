use std::{
    fs, io,
    path::{Path, PathBuf},
};

use animestan_core::{AppConfig, CoreResult};
use anyhow::{Context, anyhow};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

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

#[derive(Clone, Debug)]
struct HeatmapPalette {
    watched: Color,
}

#[derive(Clone, Debug)]
struct TextStyle {
    fg: Color,
    bg: Option<Color>,
}

#[derive(Clone, Debug, Deserialize)]
struct ThemeConfig {
    panels: PanelConfig,
    titles: ColorConfig,
    items: ColorConfig,
    selected_item: ColorConfig,
    heatmap: HeatmapConfig,
    non_interactive: ColorConfig,
}

#[derive(Clone, Debug, Deserialize)]
struct PanelConfig {
    border: String,
    focused_border: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ColorConfig {
    fg: String,
    #[serde(default)]
    bg: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct HeatmapConfig {
    watched: String,
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
    pub fn heatmap_color(&self) -> Color {
        self.heatmap.watched
    }

    pub fn load() -> CoreResult<Self> {
        let path = Self::config_path();
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

    fn config_path() -> PathBuf {
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
                watched: parse_color_string(&parsed.heatmap.watched)?,
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
    if let Some(color) = parse_hex_color(trimmed) {
        return Ok(color);
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

fn parse_hex_color(value: &str) -> Option<Color> {
    let normalized = value.trim();
    let normalized = normalized.strip_prefix('#').unwrap_or(normalized);
    if normalized.len() != 6 {
        return None;
    }

    let decoded = u32::from_str_radix(normalized, 16).ok()?;
    let red = ((decoded >> 16) & 0xFF) as u8;
    let green = ((decoded >> 8) & 0xFF) as u8;
    let blue = (decoded & 0xFF) as u8;
    Some(Color::Rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            parse_color_string(" #A6D189 ").expect("hex color should parse"),
            Color::Rgb(166, 209, 137)
        );
    }

    #[test]
    fn ignores_legacy_heatmap_colors() {
        let theme = Theme::from_toml(
            r##"
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
"##,
        )
        .expect("legacy heatmap fields should be ignored");

        assert_eq!(theme.heatmap_color(), Color::Rgb(166, 209, 137));
    }
}
