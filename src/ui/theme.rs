use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const HIGHLIGHT_SYMBOL: &str = "► ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeChoice {
    Auto,
    Dark,
    Light,
    Custom(String),
}

impl ThemeChoice {
    pub fn resolve(&self) -> Theme {
        match self {
            ThemeChoice::Dark => Theme::dark(),
            ThemeChoice::Light => Theme::light(),
            ThemeChoice::Custom(name) => {
                let path = themes_dir().join(format!("{}.toml", name));
                Theme::from_toml_file(&path).unwrap_or_else(|| Theme::dark())
            }
            ThemeChoice::Auto => match std::env::var("COLORFGBG") {
                Ok(val) => {
                    let bg_val = val
                        .split(|c| c == ':' || c == ';')
                        .last()
                        .and_then(|s| s.parse::<u8>().ok());
                    match bg_val {
                        Some(b) if b < 8 => Theme::dark(),
                        _ => Theme::light(),
                    }
                }
                Err(_) => Theme::dark(),
            },
        }
    }

    pub fn label(&self) -> &str {
        match self {
            ThemeChoice::Auto => "Auto",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
            ThemeChoice::Custom(name) => name.as_str(),
        }
    }

    pub fn ser_name(&self) -> &str {
        match self {
            ThemeChoice::Auto => "Auto",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
            ThemeChoice::Custom(name) => name.as_str(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Auto" | "auto" => ThemeChoice::Auto,
            "Dark" | "dark" => ThemeChoice::Dark,
            "Light" | "light" => ThemeChoice::Light,
            name => ThemeChoice::Custom(name.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    // Panels
    pub panel_active_border: Color,
    pub panel_inactive_border: Color,
    pub directory_icon: Color,
    pub file_name: Color,
    pub marked_entry: Color,
    pub mark_indicator: Color,
    pub size_text: Color,
    pub date_text: Color,
    pub permission_text: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub highlight_symbol: &'static str,

    // Status bar
    pub hint_badge_bg: Color,
    pub hint_badge_fg: Color,
    pub hint_badge_danger_bg: Color,
    pub hint_label: Color,
    pub status_message: Color,
    pub hint_bar_bg: Color,
    pub transfer_filled_fg: Color,
    pub transfer_empty_bg: Color,
    pub upload_bar: Color,
    pub download_bar: Color,
    pub transfer_row_bg: Color,
    pub filename_text: Color,

    // Dialogs — borders
    pub dialog_active_border: Color,
    pub dialog_inactive_border: Color,
    pub dialog_warning_border: Color,
    pub dialog_error_border: Color,
    pub dialog_success_border: Color,

    // Dialogs — text
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_active: Color,
    pub text_inactive: Color,
    pub cursor_bg: Color,
    pub cursor_fg: Color,
    pub toggle_on: Color,
    pub toggle_off: Color,
    pub text_danger: Color,
    pub text_success: Color,
    pub text_warning: Color,
    pub text_info: Color,
    pub profile_active: Color,
    pub badge_bg: Color,
    pub badge_fg: Color,

    // Shell
    pub shell_cursor_bg: Color,
    pub shell_cursor_fg: Color,
    pub shell_output_bg: Color,
    pub shell_label: Color,
    pub highlight_primary_bg: Color,
    pub highlight_primary_fg: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            panel_active_border: Color::Cyan,
            panel_inactive_border: Color::DarkGray,
            directory_icon: Color::Yellow,
            file_name: Color::White,
            marked_entry: Color::Yellow,
            mark_indicator: Color::Yellow,
            size_text: Color::Gray,
            date_text: Color::DarkGray,
            permission_text: Color::DarkGray,
            highlight_bg: Color::Blue,
            highlight_fg: Color::White,
            highlight_symbol: HIGHLIGHT_SYMBOL,

            hint_badge_bg: Color::DarkGray,
            hint_badge_fg: Color::White,
            hint_badge_danger_bg: Color::Red,
            hint_label: Color::White,
            status_message: Color::Yellow,
            hint_bar_bg: Color::Black,
            transfer_filled_fg: Color::Black,
            transfer_empty_bg: Color::DarkGray,
            upload_bar: Color::Green,
            download_bar: Color::Cyan,
            transfer_row_bg: Color::Black,
            filename_text: Color::White,

            dialog_active_border: Color::Cyan,
            dialog_inactive_border: Color::DarkGray,
            dialog_warning_border: Color::Yellow,
            dialog_error_border: Color::Red,
            dialog_success_border: Color::Green,

            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_muted: Color::DarkGray,
            text_active: Color::White,
            text_inactive: Color::Gray,
            cursor_bg: Color::Cyan,
            cursor_fg: Color::Black,
            toggle_on: Color::Green,
            toggle_off: Color::DarkGray,
            text_danger: Color::Red,
            text_success: Color::Green,
            text_warning: Color::Yellow,
            text_info: Color::Blue,
            profile_active: Color::Green,
            badge_bg: Color::DarkGray,
            badge_fg: Color::White,

            shell_cursor_bg: Color::White,
            shell_cursor_fg: Color::Black,
            shell_output_bg: Color::Black,
            shell_label: Color::Yellow,

            highlight_primary_bg: Color::Blue,
            highlight_primary_fg: Color::White,
        }
    }

    pub fn light() -> Self {
        Self {
            panel_active_border: Color::Blue,
            panel_inactive_border: Color::Gray,
            directory_icon: Color::Blue,
            file_name: Color::Black,
            marked_entry: Color::Blue,
            mark_indicator: Color::Blue,
            size_text: Color::DarkGray,
            date_text: Color::Gray,
            permission_text: Color::Gray,
            highlight_bg: Color::Cyan,
            highlight_fg: Color::Black,
            highlight_symbol: HIGHLIGHT_SYMBOL,

            hint_badge_bg: Color::Gray,
            hint_badge_fg: Color::White,
            hint_badge_danger_bg: Color::Red,
            hint_label: Color::Black,
            status_message: Color::Blue,
            hint_bar_bg: Color::White,
            transfer_filled_fg: Color::White,
            transfer_empty_bg: Color::Gray,
            upload_bar: Color::Green,
            download_bar: Color::Blue,
            transfer_row_bg: Color::White,
            filename_text: Color::Black,

            dialog_active_border: Color::Blue,
            dialog_inactive_border: Color::Gray,
            dialog_warning_border: Color::Blue,
            dialog_error_border: Color::Red,
            dialog_success_border: Color::Green,

            text_primary: Color::Black,
            text_secondary: Color::DarkGray,
            text_muted: Color::Gray,
            text_active: Color::Black,
            text_inactive: Color::DarkGray,
            cursor_bg: Color::Blue,
            cursor_fg: Color::White,
            toggle_on: Color::Green,
            toggle_off: Color::Gray,
            text_danger: Color::Red,
            text_success: Color::Green,
            text_warning: Color::Blue,
            text_info: Color::Blue,
            profile_active: Color::Green,
            badge_bg: Color::Gray,
            badge_fg: Color::White,

            shell_cursor_bg: Color::Black,
            shell_cursor_fg: Color::White,
            shell_output_bg: Color::White,
            shell_label: Color::Blue,

            highlight_primary_bg: Color::Cyan,
            highlight_primary_fg: Color::Black,
        }
    }

    pub fn from_toml_file(path: &std::path::Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let tf: ThemeToml = toml::from_str(&content).ok()?;
        tf.to_theme()
    }

    fn to_toml_string(&self, name: &str) -> String {
        let tf = ThemeToml::from_theme(self, name);
        toml::to_string_pretty(&tf).unwrap_or_default()
    }

    /// Returns a copy of the dark theme as a starting point for custom themes.
    pub fn custom_template() -> Self {
        Self::dark()
    }
}

// ---------------------------------------------------------------------------
// TOML helper — maps Theme <-> serializable struct with string colour names
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThemeToml {
    name: String,
    panel_active_border: String,
    panel_inactive_border: String,
    directory_icon: String,
    file_name: String,
    marked_entry: String,
    mark_indicator: String,
    size_text: String,
    date_text: String,
    permission_text: String,
    highlight_bg: String,
    highlight_fg: String,
    hint_badge_bg: String,
    hint_badge_fg: String,
    hint_badge_danger_bg: String,
    hint_label: String,
    status_message: String,
    hint_bar_bg: String,
    transfer_filled_fg: String,
    transfer_empty_bg: String,
    upload_bar: String,
    download_bar: String,
    transfer_row_bg: String,
    filename_text: String,
    dialog_active_border: String,
    dialog_inactive_border: String,
    dialog_warning_border: String,
    dialog_error_border: String,
    dialog_success_border: String,
    text_primary: String,
    text_secondary: String,
    text_muted: String,
    text_active: String,
    text_inactive: String,
    cursor_bg: String,
    cursor_fg: String,
    toggle_on: String,
    toggle_off: String,
    text_danger: String,
    text_success: String,
    text_warning: String,
    text_info: String,
    profile_active: String,
    badge_bg: String,
    badge_fg: String,
    shell_cursor_bg: String,
    shell_cursor_fg: String,
    shell_output_bg: String,
    shell_label: String,
    highlight_primary_bg: String,
    highlight_primary_fg: String,
}

impl ThemeToml {
    fn from_theme(t: &Theme, name: &str) -> Self {
        Self {
            name: name.to_string(),
            panel_active_border: color_name(t.panel_active_border),
            panel_inactive_border: color_name(t.panel_inactive_border),
            directory_icon: color_name(t.directory_icon),
            file_name: color_name(t.file_name),
            marked_entry: color_name(t.marked_entry),
            mark_indicator: color_name(t.mark_indicator),
            size_text: color_name(t.size_text),
            date_text: color_name(t.date_text),
            permission_text: color_name(t.permission_text),
            highlight_bg: color_name(t.highlight_bg),
            highlight_fg: color_name(t.highlight_fg),
            hint_badge_bg: color_name(t.hint_badge_bg),
            hint_badge_fg: color_name(t.hint_badge_fg),
            hint_badge_danger_bg: color_name(t.hint_badge_danger_bg),
            hint_label: color_name(t.hint_label),
            status_message: color_name(t.status_message),
            hint_bar_bg: color_name(t.hint_bar_bg),
            transfer_filled_fg: color_name(t.transfer_filled_fg),
            transfer_empty_bg: color_name(t.transfer_empty_bg),
            upload_bar: color_name(t.upload_bar),
            download_bar: color_name(t.download_bar),
            transfer_row_bg: color_name(t.transfer_row_bg),
            filename_text: color_name(t.filename_text),
            dialog_active_border: color_name(t.dialog_active_border),
            dialog_inactive_border: color_name(t.dialog_inactive_border),
            dialog_warning_border: color_name(t.dialog_warning_border),
            dialog_error_border: color_name(t.dialog_error_border),
            dialog_success_border: color_name(t.dialog_success_border),
            text_primary: color_name(t.text_primary),
            text_secondary: color_name(t.text_secondary),
            text_muted: color_name(t.text_muted),
            text_active: color_name(t.text_active),
            text_inactive: color_name(t.text_inactive),
            cursor_bg: color_name(t.cursor_bg),
            cursor_fg: color_name(t.cursor_fg),
            toggle_on: color_name(t.toggle_on),
            toggle_off: color_name(t.toggle_off),
            text_danger: color_name(t.text_danger),
            text_success: color_name(t.text_success),
            text_warning: color_name(t.text_warning),
            text_info: color_name(t.text_info),
            profile_active: color_name(t.profile_active),
            badge_bg: color_name(t.badge_bg),
            badge_fg: color_name(t.badge_fg),
            shell_cursor_bg: color_name(t.shell_cursor_bg),
            shell_cursor_fg: color_name(t.shell_cursor_fg),
            shell_output_bg: color_name(t.shell_output_bg),
            shell_label: color_name(t.shell_label),
            highlight_primary_bg: color_name(t.highlight_primary_bg),
            highlight_primary_fg: color_name(t.highlight_primary_fg),
        }
    }

    fn to_theme(&self) -> Option<Theme> {
        Some(Theme {
            panel_active_border: parse_color(&self.panel_active_border)?,
            panel_inactive_border: parse_color(&self.panel_inactive_border)?,
            directory_icon: parse_color(&self.directory_icon)?,
            file_name: parse_color(&self.file_name)?,
            marked_entry: parse_color(&self.marked_entry)?,
            mark_indicator: parse_color(&self.mark_indicator)?,
            size_text: parse_color(&self.size_text)?,
            date_text: parse_color(&self.date_text)?,
            permission_text: parse_color(&self.permission_text)?,
            highlight_bg: parse_color(&self.highlight_bg)?,
            highlight_fg: parse_color(&self.highlight_fg)?,
            highlight_symbol: HIGHLIGHT_SYMBOL,

            hint_badge_bg: parse_color(&self.hint_badge_bg)?,
            hint_badge_fg: parse_color(&self.hint_badge_fg)?,
            hint_badge_danger_bg: parse_color(&self.hint_badge_danger_bg)?,
            hint_label: parse_color(&self.hint_label)?,
            status_message: parse_color(&self.status_message)?,
            hint_bar_bg: parse_color(&self.hint_bar_bg)?,
            transfer_filled_fg: parse_color(&self.transfer_filled_fg)?,
            transfer_empty_bg: parse_color(&self.transfer_empty_bg)?,
            upload_bar: parse_color(&self.upload_bar)?,
            download_bar: parse_color(&self.download_bar)?,
            transfer_row_bg: parse_color(&self.transfer_row_bg)?,
            filename_text: parse_color(&self.filename_text)?,

            dialog_active_border: parse_color(&self.dialog_active_border)?,
            dialog_inactive_border: parse_color(&self.dialog_inactive_border)?,
            dialog_warning_border: parse_color(&self.dialog_warning_border)?,
            dialog_error_border: parse_color(&self.dialog_error_border)?,
            dialog_success_border: parse_color(&self.dialog_success_border)?,

            text_primary: parse_color(&self.text_primary)?,
            text_secondary: parse_color(&self.text_secondary)?,
            text_muted: parse_color(&self.text_muted)?,
            text_active: parse_color(&self.text_active)?,
            text_inactive: parse_color(&self.text_inactive)?,
            cursor_bg: parse_color(&self.cursor_bg)?,
            cursor_fg: parse_color(&self.cursor_fg)?,
            toggle_on: parse_color(&self.toggle_on)?,
            toggle_off: parse_color(&self.toggle_off)?,
            text_danger: parse_color(&self.text_danger)?,
            text_success: parse_color(&self.text_success)?,
            text_warning: parse_color(&self.text_warning)?,
            text_info: parse_color(&self.text_info)?,
            profile_active: parse_color(&self.profile_active)?,
            badge_bg: parse_color(&self.badge_bg)?,
            badge_fg: parse_color(&self.badge_fg)?,

            shell_cursor_bg: parse_color(&self.shell_cursor_bg)?,
            shell_cursor_fg: parse_color(&self.shell_cursor_fg)?,
            shell_output_bg: parse_color(&self.shell_output_bg)?,
            shell_label: parse_color(&self.shell_label)?,
            highlight_primary_bg: parse_color(&self.highlight_primary_bg)?,
            highlight_primary_fg: parse_color(&self.highlight_primary_fg)?,
        })
    }
}

// ---------------------------------------------------------------------------
// Color <-> string helpers
// ---------------------------------------------------------------------------

fn parse_color(s: &str) -> Option<Color> {
    match s {
        "Black" => Some(Color::Black),
        "Red" => Some(Color::Red),
        "Green" => Some(Color::Green),
        "Yellow" => Some(Color::Yellow),
        "Blue" => Some(Color::Blue),
        "Magenta" => Some(Color::Magenta),
        "Cyan" => Some(Color::Cyan),
        "White" => Some(Color::White),
        "Gray" => Some(Color::Gray),
        "DarkGray" => Some(Color::DarkGray),
        "LightRed" => Some(Color::LightRed),
        "LightGreen" => Some(Color::LightGreen),
        "LightYellow" => Some(Color::LightYellow),
        "LightBlue" => Some(Color::LightBlue),
        "LightMagenta" => Some(Color::LightMagenta),
        "LightCyan" => Some(Color::LightCyan),
        "LightWhite" | "LightGray" => Some(Color::White),
        _ => None,
    }
}

fn color_name(c: Color) -> String {
    match c {
        Color::Black => "Black",
        Color::Red => "Red",
        Color::Green => "Green",
        Color::Yellow => "Yellow",
        Color::Blue => "Blue",
        Color::Magenta => "Magenta",
        Color::Cyan => "Cyan",
        Color::White => "White",
        Color::Gray => "Gray",
        Color::DarkGray => "DarkGray",
        Color::LightRed => "LightRed",
        Color::LightGreen => "LightGreen",
        Color::LightYellow => "LightYellow",
        Color::LightBlue => "LightBlue",
        Color::LightMagenta => "LightMagenta",
        Color::LightCyan => "LightCyan",
        _ => "White",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Persistence: settings.toml + theme files
// ---------------------------------------------------------------------------

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config").join("vela")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.toml")
}

fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

/// Return file names (without .toml) of custom themes in the themes directory.
pub fn custom_theme_names() -> Vec<String> {
    let dir = themes_dir();
    if !dir.is_dir() {
        return vec![];
    }
    let mut names: Vec<String> = vec![];
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let lower = stem.to_lowercase();
                    if lower != "dark" && lower != "light" {
                        names.push(stem.to_string());
                    }
                }
            }
        }
    }
    names.sort();
    names
}

pub fn load_theme_choice() -> ThemeChoice {
    let path = settings_path();
    if !path.exists() {
        return ThemeChoice::Auto;
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return ThemeChoice::Auto,
    };
    // Simple TOML parse: theme = "value"
    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("theme = ") {
            let v = val.trim().trim_matches('"').trim_matches('\'');
            return ThemeChoice::from_str(v);
        }
    }
    ThemeChoice::Auto
}

pub fn save_theme_choice(choice: &ThemeChoice) {
    let content = format!("theme = \"{}\"\n", choice.ser_name());
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, content);
}

/// Ensure the theme template files exist in ~/.config/vela/themes/.
/// Called once on startup. Does not overwrite existing files.
pub fn ensure_themes() {
    let dir = themes_dir();
    let _ = fs::create_dir_all(&dir);

    let pairs: [(&str, &Theme); 3] = [
        ("dark", &Theme::dark()),
        ("light", &Theme::light()),
        ("custom", &Theme::custom_template()),
    ];

    for (name, theme) in &pairs {
        let path = dir.join(format!("{}.toml", name));
        if !path.exists() {
            let content = theme.to_toml_string(name);
            let _ = fs::write(&path, content);
        }
    }
}