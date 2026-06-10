use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const HIGHLIGHT_SYMBOL: &str = "► ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    Auto,
    Dark,
    Light,
}

impl ThemeChoice {
    pub fn resolve(&self) -> Theme {
        match self {
            ThemeChoice::Dark => Theme::dark(),
            ThemeChoice::Light => Theme::light(),
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

    pub fn label(&self) -> &'static str {
        match self {
            ThemeChoice::Auto => "Auto",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            ThemeChoice::Auto => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::Auto,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    theme: ThemeChoice,
}

fn settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("vela")
        .join("settings.toml")
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
    match toml::from_str::<Settings>(&content) {
        Ok(s) => s.theme,
        Err(_) => ThemeChoice::Auto,
    }
}

pub fn save_theme_choice(choice: ThemeChoice) {
    let settings = Settings { theme: choice };
    let content = match toml::to_string_pretty(&settings) {
        Ok(c) => c,
        Err(_) => return,
    };
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, content);
}