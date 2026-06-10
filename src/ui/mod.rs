pub mod dialogs;
pub mod panels;
pub mod statusbar;
pub mod theme;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::app::App;
use dialogs::{
    render_delete_dialog, render_help_dialog, render_host_key_dialog, render_mkdir_dialog,
    render_password_dialog, render_permission_dialog, render_profile_dialog, render_rename_dialog,
    render_shell_dialog,
};
use panels::render_panels;
use statusbar::render_statusbar;

/// Top-level render function called each frame.
pub fn render(frame: &mut Frame, app: &App) {
    let theme = app.theme_choice.resolve();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // panels take remaining space
            Constraint::Length(2), // status bar (2 lines: gauge + file info)
        ])
        .split(frame.area());

    render_panels(frame, app, chunks[0], &theme);

    render_statusbar(
        frame,
        chunks[1],
        app.is_connected(),
        app.status_message.as_deref(),
        app.upload_progress.as_ref(),
        app.download_progress.as_ref(),
        &theme,
    );

    // Dialog overlays — rendered last so they appear on top
    if let Some(ref dialog) = app.profile_dialog {
        render_profile_dialog(frame, dialog, &theme);
    }
    if let Some(ref dlg) = app.password_dialog {
        render_password_dialog(frame, dlg, &theme);
    }
    if let Some(ref dlg) = app.rename_dialog {
        render_rename_dialog(frame, dlg, &theme);
    }
    if let Some(ref dlg) = app.mkdir_dialog {
        render_mkdir_dialog(frame, dlg, &theme);
    }
    if let Some(ref dlg) = app.delete_dialog {
        render_delete_dialog(frame, dlg, &theme);
    }
    if let Some(ref dlg) = app.shell_dialog {
        render_shell_dialog(frame, dlg, &app.left.path, &theme);
    }
    if let Some(ref dlg) = app.permission_dialog {
        render_permission_dialog(frame, dlg, &theme);
    }
    if let Some(ref dlg) = app.host_key_dialog {
        render_host_key_dialog(frame, dlg, &theme);
    }
    // Help overlay on top of everything else
    if app.help_visible {
        render_help_dialog(frame, &theme);
    }
}
