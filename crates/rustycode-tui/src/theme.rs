//! Theme management for the TUI
//!
//! Provides comprehensive theme switching with color palettes,
//! built-in themes, and custom theme support.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Legacy simple theme enum for backward compatibility
#[derive(Clone, Copy, PartialEq, Debug)]
#[non_exhaustive]
pub enum LegacyTheme {
    Dark,
    Light,
}

impl LegacyTheme {
    pub fn name(&self) -> &str {
        match self {
            LegacyTheme::Dark => "Dark",
            LegacyTheme::Light => "Light",
        }
    }

    pub fn toggle(&self) -> LegacyTheme {
        match self {
            LegacyTheme::Dark => LegacyTheme::Light,
            LegacyTheme::Light => LegacyTheme::Dark,
        }
    }

    /// Get the default theme
    pub fn default_theme() -> Self {
        LegacyTheme::Dark
    }
}

/// Comprehensive theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub colors: ColorPalette,
}

/// Color palette for theme customization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPalette {
    pub background: String,
    pub foreground: String,
    pub primary: String,   // Headers, titles
    pub secondary: String, // Borders
    pub accent: String,    // Buttons, highlights
    pub success: String,   // Good news
    pub warning: String,   // Warnings
    pub error: String,     // Errors
    pub muted: String,     // Disabled text
    pub user_bar: String,  // Vertical bar for user messages (pink)
    pub ai_bar: String,    // Vertical bar for AI messages (cyan)
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "midnight-rust".to_string(),
            colors: ColorPalette {
                background: "#2e3440".to_string(),
                foreground: "#eceff4".to_string(),
                primary: "#5e81ac".to_string(),
                secondary: "#81a1c4".to_string(),
                accent: "#88c0d0".to_string(),
                success: "#a3be8c".to_string(),
                warning: "#ebcb8b".to_string(),
                error: "#bf616a".to_string(),
                muted: "#4c566a".to_string(),
                user_bar: "#b48ead".to_string(),
                ai_bar: "#8fbcbb".to_string(),
            },
        }
    }
}

/// Get all built-in themes (16+ professional themes)
pub fn builtin_themes() -> Vec<Theme> {
    vec![
        // === Midnight Rust (new default) ===
        Theme {
            name: "midnight-rust".to_string(),
            colors: ColorPalette {
                background: "#2e3440".to_string(),
                foreground: "#eceff4".to_string(),
                primary: "#5e81ac".to_string(),
                secondary: "#81a1c4".to_string(),
                accent: "#88c0d0".to_string(),
                success: "#a3be8c".to_string(),
                warning: "#ebcb8b".to_string(),
                error: "#bf616a".to_string(),
                muted: "#4c566a".to_string(),
                user_bar: "#b48ead".to_string(),
                ai_bar: "#8fbcbb".to_string(),
            },
        },
        // === Community-inspired themes ===
        Theme {
            name: "oc-1".to_string(),
            colors: ColorPalette {
                background: "#f8f7f7".to_string(),
                foreground: "#8e8b8b".to_string(),
                primary: "#dcde8d".to_string(),
                secondary: "#d0d0d0".to_string(),
                accent: "#034cff".to_string(),
                success: "#12c905".to_string(),
                warning: "#ffdc17".to_string(),
                error: "#fc533a".to_string(),
                muted: "#c0c0c0".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "tokyo-night".to_string(),
            colors: ColorPalette {
                background: "#1a1b26".to_string(),
                foreground: "#a9b1d6".to_string(),
                primary: "#7aa2f7".to_string(),
                secondary: "#414868".to_string(),
                accent: "#bb9af7".to_string(),
                success: "#9ece6a".to_string(),
                warning: "#e0af68".to_string(),
                error: "#f7768e".to_string(),
                muted: "#565f89".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "dracula".to_string(),
            colors: ColorPalette {
                background: "#282a36".to_string(),
                foreground: "#f8f8f2".to_string(),
                primary: "#bd93f9".to_string(),
                secondary: "#44475a".to_string(),
                accent: "#ff79c6".to_string(),
                success: "#50fa7b".to_string(),
                warning: "#ffb86c".to_string(),
                error: "#ff5555".to_string(),
                muted: "#6272a4".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "monokai".to_string(),
            colors: ColorPalette {
                background: "#272822".to_string(),
                foreground: "#f8f8f2".to_string(),
                primary: "#a6e22e".to_string(),
                secondary: "#3e3d32".to_string(),
                accent: "#f92672".to_string(),
                success: "#a6e22e".to_string(),
                warning: "#e6db74".to_string(),
                error: "#f92672".to_string(),
                muted: "#75715e".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "nord".to_string(),
            colors: ColorPalette {
                background: "#2e3440".to_string(),
                foreground: "#eceff4".to_string(),
                primary: "#88c0d0".to_string(),
                secondary: "#4c566a".to_string(),
                accent: "#81a1c1".to_string(),
                success: "#a3be8c".to_string(),
                warning: "#ebcb8b".to_string(),
                error: "#bf616a".to_string(),
                muted: "#4c566a".to_string(),
                user_bar: "#b48ead".to_string(),
                ai_bar: "#8fbcbb".to_string(),
            },
        },
        Theme {
            name: "gruvbox-dark".to_string(),
            colors: ColorPalette {
                background: "#282828".to_string(),
                foreground: "#ebdbb2".to_string(),
                primary: "#83a598".to_string(),
                secondary: "#504945".to_string(),
                accent: "#d3869b".to_string(),
                success: "#b8bb26".to_string(),
                warning: "#fabd2f".to_string(),
                error: "#fb4934".to_string(),
                muted: "#665c54".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "gruvbox-light".to_string(),
            colors: ColorPalette {
                background: "#fbf1c7".to_string(),
                foreground: "#3c3836".to_string(),
                primary: "#076678".to_string(),
                secondary: "#d5c4a1".to_string(),
                accent: "#b16286".to_string(),
                success: "#98971a".to_string(),
                warning: "#d79921".to_string(),
                error: "#cc241d".to_string(),
                muted: "#928374".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "catppuccin-mocha".to_string(),
            colors: ColorPalette {
                background: "#1e1e2e".to_string(),
                foreground: "#cdd6f4".to_string(),
                primary: "#89b4fa".to_string(),
                secondary: "#45475a".to_string(),
                accent: "#cba6f7".to_string(),
                success: "#a6e3a1".to_string(),
                warning: "#f9e2af".to_string(),
                error: "#f38ba8".to_string(),
                muted: "#6c7086".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "catppuccin-latte".to_string(),
            colors: ColorPalette {
                background: "#eff1f5".to_string(),
                foreground: "#4c4f69".to_string(),
                primary: "#1e66f5".to_string(),
                secondary: "#ccd0da".to_string(),
                accent: "#8839ef".to_string(),
                success: "#40a02b".to_string(),
                warning: "#df8e1d".to_string(),
                error: "#d20f39".to_string(),
                muted: "#9ca0b0".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "rose-pine".to_string(),
            colors: ColorPalette {
                background: "#191724".to_string(),
                foreground: "#e0def4".to_string(),
                primary: "#c4a7e7".to_string(),
                secondary: "#26233a".to_string(),
                accent: "#9ccfd8".to_string(),
                success: "#ebbcba".to_string(),
                warning: "#f6c177".to_string(),
                error: "#eb6f92".to_string(),
                muted: "#6e6a86".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "rose-pine-dawn".to_string(),
            colors: ColorPalette {
                background: "#faf4ed".to_string(),
                foreground: "#575279".to_string(),
                primary: "#286983".to_string(),
                secondary: "#f2e9de".to_string(),
                accent: "#ea9d34".to_string(),
                success: "#56949f".to_string(),
                warning: "#d7827e".to_string(),
                error: "#b4637a".to_string(),
                muted: "#9893a5".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "solarized-dark".to_string(),
            colors: ColorPalette {
                background: "#002b36".to_string(),
                foreground: "#839496".to_string(),
                primary: "#268bd2".to_string(),
                secondary: "#073642".to_string(),
                accent: "#6c71c4".to_string(),
                success: "#859900".to_string(),
                warning: "#b58900".to_string(),
                error: "#dc322f".to_string(),
                muted: "#586e75".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "solarized-light".to_string(),
            colors: ColorPalette {
                background: "#fdf6e3".to_string(),
                foreground: "#657b83".to_string(),
                primary: "#268bd2".to_string(),
                secondary: "#eee8d5".to_string(),
                accent: "#6c71c4".to_string(),
                success: "#859900".to_string(),
                warning: "#b58900".to_string(),
                error: "#dc322f".to_string(),
                muted: "#93a1a1".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "github-dark".to_string(),
            colors: ColorPalette {
                background: "#0d1117".to_string(),
                foreground: "#c9d1d9".to_string(),
                primary: "#58a6ff".to_string(),
                secondary: "#21262d".to_string(),
                accent: "#bc8cff".to_string(),
                success: "#3fb950".to_string(),
                warning: "#d29922".to_string(),
                error: "#f85149".to_string(),
                muted: "#8b949e".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "github-light".to_string(),
            colors: ColorPalette {
                background: "#ffffff".to_string(),
                foreground: "#24292f".to_string(),
                primary: "#0969da".to_string(),
                secondary: "#f6f8fa".to_string(),
                accent: "#8250df".to_string(),
                success: "#1a7f37".to_string(),
                warning: "#9a6700".to_string(),
                error: "#cf222e".to_string(),
                muted: "#6e7781".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "vscode-dark".to_string(),
            colors: ColorPalette {
                background: "#1e1e1e".to_string(),
                foreground: "#d4d4d4".to_string(),
                primary: "#569cd6".to_string(),
                secondary: "#252526".to_string(),
                accent: "#c586c0".to_string(),
                success: "#6a9955".to_string(),
                warning: "#dcdcaa".to_string(),
                error: "#f44747".to_string(),
                muted: "#858585".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "vscode-light".to_string(),
            colors: ColorPalette {
                background: "#ffffff".to_string(),
                foreground: "#000000".to_string(),
                primary: "#0066b8".to_string(),
                secondary: "#f3f3f3".to_string(),
                accent: "#a31515".to_string(),
                success: "#008000".to_string(),
                warning: "#795e26".to_string(),
                error: "#cd3131".to_string(),
                muted: "#666666".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "atom-one-dark".to_string(),
            colors: ColorPalette {
                background: "#282c34".to_string(),
                foreground: "#abb2bf".to_string(),
                primary: "#61afef".to_string(),
                secondary: "#21252b".to_string(),
                accent: "#c678dd".to_string(),
                success: "#98c379".to_string(),
                warning: "#e5c07b".to_string(),
                error: "#e06c75".to_string(),
                muted: "#5c6370".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "atom-one-light".to_string(),
            colors: ColorPalette {
                background: "#fafafa".to_string(),
                foreground: "#383a42".to_string(),
                primary: "#4078f2".to_string(),
                secondary: "#e5e5e6".to_string(),
                accent: "#a626a4".to_string(),
                success: "#50a14f".to_string(),
                warning: "#986801".to_string(),
                error: "#ca1243".to_string(),
                muted: "#a0a1a7".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "material-dark".to_string(),
            colors: ColorPalette {
                background: "#263238".to_string(),
                foreground: "#eeffff".to_string(),
                primary: "#82b1ff".to_string(),
                secondary: "#37474f".to_string(),
                accent: "#c792ea".to_string(),
                success: "#c3e88d".to_string(),
                warning: "#ffcb6b".to_string(),
                error: "#f07178".to_string(),
                muted: "#546e7a".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "palenight".to_string(),
            colors: ColorPalette {
                background: "#292d3e".to_string(),
                foreground: "#bfc7d5".to_string(),
                primary: "#89ddff".to_string(),
                secondary: "#444267".to_string(),
                accent: "#f78c6c".to_string(),
                success: "#c3e88d".to_string(),
                warning: "#ffcb6b".to_string(),
                error: "#ff5370".to_string(),
                muted: "#676e95".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "ayu-dark".to_string(),
            colors: ColorPalette {
                background: "#0f1419".to_string(),
                foreground: "#e6e1cf".to_string(),
                primary: "#39bae6".to_string(),
                secondary: "#1f2428".to_string(),
                accent: "#f07178".to_string(),
                success: "#b8cc52".to_string(),
                warning: "#e6b450".to_string(),
                error: "#ff3333".to_string(),
                muted: "#5c6773".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "night-owl".to_string(),
            colors: ColorPalette {
                background: "#011627".to_string(),
                foreground: "#d6deeb".to_string(),
                primary: "#82aaff".to_string(),
                secondary: "#0d1f33".to_string(),
                accent: "#c792ea".to_string(),
                success: "#c5e478".to_string(),
                warning: "#ffcb6b".to_string(),
                error: "#ff5874".to_string(),
                muted: "#637777".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "vesper".to_string(),
            colors: ColorPalette {
                background: "#101317".to_string(),
                foreground: "#b3b9c5".to_string(),
                primary: "#68d0f8".to_string(),
                secondary: "#1a1e25".to_string(),
                accent: "#c77bbf".to_string(),
                success: "#a5d3a5".to_string(),
                warning: "#e3c78a".to_string(),
                error: "#e27e8d".to_string(),
                muted: "#4b5563".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
        Theme {
            name: "carbonfox".to_string(),
            colors: ColorPalette {
                background: "#161616".to_string(),
                foreground: "#f2f4f8".to_string(),
                primary: "#78a9ff".to_string(),
                secondary: "#282828".to_string(),
                accent: "#df81e6".to_string(),
                success: "#78dba9".to_string(),
                warning: "#ffd385".to_string(),
                error: "#ff6a7b".to_string(),
                muted: "#8e8e8e".to_string(),
                user_bar: "#ff69b4".to_string(),
                ai_bar: "#00ffff".to_string(),
            },
        },
    ]
}

impl Theme {
    /// Load theme from file
    pub fn load_from_file(path: &PathBuf) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read theme file: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse theme: {}", e))
    }

    /// Save theme to file
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create theme directory: {}", e))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize theme: {}", e))?;
        std::fs::write(path, content).map_err(|e| format!("Failed to write theme file: {}", e))?;
        Ok(())
    }

    /// Find theme by name
    pub fn find_by_name(name: &str) -> Option<Self> {
        builtin_themes().into_iter().find(|t| t.name == name)
    }
}

/// Parse hex color string to ratatui Color
pub fn parse_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');

    if hex.len() != 6 {
        return Color::Reset; // Use Reset instead of Default
    }

    match (
        u8::from_str_radix(&hex[0..2], 16),
        u8::from_str_radix(&hex[2..4], 16),
        u8::from_str_radix(&hex[4..6], 16),
    ) {
        (Ok(r), Ok(g), Ok(b)) => Color::Rgb(r, g, b),
        _ => Color::Reset,
    }
}

/// Color mapping for UI elements
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub background: Color,
    pub foreground: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub muted: Color,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            background: Color::Reset,
            foreground: Color::Reset,
            primary: Color::Cyan,
            secondary: Color::Magenta,
            accent: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
        }
    }
}

impl From<&Theme> for ThemeColors {
    fn from(theme: &Theme) -> Self {
        Self {
            background: parse_color(&theme.colors.background),
            foreground: parse_color(&theme.colors.foreground),
            primary: parse_color(&theme.colors.primary),
            secondary: parse_color(&theme.colors.secondary),
            accent: parse_color(&theme.colors.accent),
            success: parse_color(&theme.colors.success),
            warning: parse_color(&theme.colors.warning),
            error: parse_color(&theme.colors.error),
            muted: parse_color(&theme.colors.muted),
        }
    }
}

/// Get platform-specific modifier key icon and name
pub fn get_mod_key() -> (&'static str, &'static str) {
    // On macOS, show Command (⌘). On other platforms, show Control (⌃).
    if cfg!(target_os = "macos") {
        ("⌘", "Cmd")
    } else {
        ("⌃", "Ctrl")
    }
}

/// Get platform-specific multiline key
pub fn get_multiline_key() -> &'static str {
    // On macOS, use Option. On other platforms, use Alt or Shift.
    if cfg!(target_os = "macos") {
        "Option"
    } else if cfg!(target_os = "windows") {
        "Shift"
    } else {
        "Alt"
    }
}

/// Format a keyboard shortcut with platform-specific modifier
pub fn fmt_shortcut(key: &str) -> String {
    let (icon, _) = get_mod_key();
    format!("{}+{}", icon, key)
}

/// Check if a theme is dark based on its background color
pub fn is_dark_theme(background: &str) -> bool {
    // Parse the background color and check if it's dark
    let color = parse_color(background);
    match color {
        Color::Rgb(r, g, b) => {
            // Calculate luminance
            let luminance = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
            luminance < 0.5
        }
        Color::Indexed(index) => index < 8, // Dark colors in 16-color palette
        Color::Reset => true,               // Default to dark for Reset
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        assert_eq!(theme.name, "midnight-rust");
    }

    #[test]
    fn test_builtin_themes() {
        let themes = builtin_themes();
        assert!(themes.len() >= 4);
        assert!(themes.iter().any(|t| t.name == "tokyo-night"));
        assert!(themes.iter().any(|t| t.name == "dracula"));
        assert!(themes.iter().any(|t| t.name == "nord"));
        assert!(themes.iter().any(|t| t.name == "gruvbox-light")); // Light theme variant
    }

    #[test]
    fn test_theme_find_by_name() {
        let theme = Theme::find_by_name("dracula");
        assert!(theme.is_some());
        assert_eq!(theme.unwrap().name, "dracula");

        let theme = Theme::find_by_name("nonexistent");
        assert!(theme.is_none());
    }

    #[test]
    fn test_parse_color() {
        let color = parse_color("#7aa2f7");
        assert!(matches!(color, Color::Rgb(122, 162, 247)));

        let color = parse_color("7aa2f7");
        assert!(matches!(color, Color::Rgb(122, 162, 247)));

        let color = parse_color("invalid");
        assert_eq!(color, Color::Reset);
    }

    #[test]
    fn test_theme_colors_conversion() {
        let theme = Theme::default();
        let colors = ThemeColors::from(&theme);
        assert!(matches!(colors.primary, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_get_mod_key() {
        let (icon, name) = get_mod_key();
        assert!(!icon.is_empty());
        assert!(!name.is_empty());
    }

    #[test]
    fn test_get_multiline_key() {
        let key = get_multiline_key();
        assert!(!key.is_empty());
        assert!(key == "Option" || key == "Alt" || key == "Shift");
    }

    #[test]
    fn test_fmt_shortcut() {
        let shortcut = fmt_shortcut("c");
        assert!(shortcut.contains("c"));
    }

    #[test]
    fn test_theme_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let theme_path = temp_dir.path().join("test-theme.toml");

        let theme = Theme::default();
        theme.save_to_file(&theme_path).unwrap();

        let loaded = Theme::load_from_file(&theme_path).unwrap();
        assert_eq!(loaded.name, theme.name);
        assert_eq!(loaded.colors.background, theme.colors.background);
    }
}
