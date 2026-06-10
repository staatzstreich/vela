use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{
    DeleteDialog, HostKeyDialog, MkdirDialog, NewProfileForm, PasswordDialog, PermissionFixDialog,
    ProfileDialog, ProfileDialogMode, RenameDialog, ShellDialog,
};
use crate::config::profiles::AuthMethod;
use crate::ui::theme::Theme;

/// Render the profile manager dialog centered on the screen.
pub fn render_profile_dialog(frame: &mut Frame, dialog: &ProfileDialog, theme: &Theme) {
    let area = centered_rect(70, 80, frame.area());
    frame.render_widget(Clear, area);

    match &dialog.mode {
        ProfileDialogMode::List => render_list(frame, dialog, area, theme),
        ProfileDialogMode::New { field } => {
            render_profile_form(frame, &dialog.form, *field, area, " Neues Profil ", theme)
        }
        ProfileDialogMode::Edit { field, .. } => {
            render_profile_form(frame, &dialog.form, *field, area, " Profil bearbeiten ", theme)
        }
        ProfileDialogMode::ConfirmDelete { index } => {
            render_confirm_delete(frame, dialog, *index, area, theme)
        }
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn render_list(frame: &mut Frame, dialog: &ProfileDialog, area: Rect, theme: &Theme) {
    let block = Block::default()
        .title(" Verbindungsprofile (F9) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_active_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // profile list
            Constraint::Length(1), // hint bar
        ])
        .split(inner);

    // Profile list
    let items: Vec<ListItem> = if dialog.store.profiles.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  Keine Profile vorhanden. N = Neu anlegen",
            Style::default().fg(theme.text_muted),
        )))]
    } else {
        dialog
            .store
            .profiles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let active_marker = if dialog.active_profile == Some(i) {
                    "● "
                } else {
                    "  "
                };
                let line = Line::from(vec![
                    Span::styled(active_marker, Style::default().fg(theme.profile_active)),
                    Span::styled(
                        format!("{:<20}", p.name),
                        Style::default().fg(theme.text_primary).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}@{}:{}", p.user, p.host, p.port),
                        Style::default().fg(theme.text_secondary),
                    ),
                    Span::styled(
                        format!("  [{}]", p.auth.as_str()),
                        Style::default().fg(theme.text_muted),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect()
    };

    let mut list_state = ListState::default();
    if !dialog.store.profiles.is_empty() {
        list_state.select(Some(dialog.list_selected));
    }

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme.highlight_primary_bg).fg(theme.highlight_primary_fg))
        .highlight_symbol("► ");

    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    // Hint bar
    let hints = Line::from(vec![
        hint_key("Enter", theme), hint_label(" Auswählen  ", theme),
        hint_key("N", theme), hint_label(" Neu  ", theme),
        hint_key("E / F2", theme), hint_label(" Bearbeiten  ", theme),
        hint_key("D", theme), hint_label(" Löschen  ", theme),
        hint_key("Esc", theme), hint_label(" Schließen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// New-profile form (dynamic layout — only visible fields get rows)
// ---------------------------------------------------------------------------

/// All possible fields with their logical index and label.
const ALL_FIELDS: &[(usize, &str)] = &[
    (0, "Name"), (1, "Host"), (2, "Port"), (3, "User"),
    (4, "Auth"), (5, "Key-Pfad"), (6, "Remote-Startpfad"),
    (7, "Lokaler Startpfad"), (8, "Passwort speichern"), (9, "Passwort"),
];

/// Return only the fields that should be visible for the current form state.
fn visible_fields(form: &NewProfileForm) -> Vec<(usize, &'static str)> {
    ALL_FIELDS
        .iter()
        .filter(|(idx, _)| match *idx {
            5 => form.auth == AuthMethod::Key,
            8 => form.auth == AuthMethod::Password,
            9 => form.auth == AuthMethod::Password && form.save_password,
            _ => true,
        })
        .copied()
        .collect()
}

fn render_profile_form(
    frame: &mut Frame,
    form: &NewProfileForm,
    active_field: usize,
    area: Rect,
    title: &str,
    theme: &Theme,
) {
    let block = Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_warning_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let vis = visible_fields(form);
    let n = vis.len();
    let mut constraints: Vec<Constraint> =
        (0..n).map(|_| Constraint::Length(3)).collect();
    constraints.push(Constraint::Min(0)); // spacer
    constraints.push(Constraint::Length(1)); // hints

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (row_idx, &(field_idx, label)) in vis.iter().enumerate() {
        let is_active = field_idx == active_field;
        let border_style = if is_active {
            Style::default().fg(theme.dialog_active_border)
        } else {
            Style::default().fg(theme.dialog_inactive_border)
        };

        match field_idx {
            4 => render_auth_toggle(frame, form, is_active, border_style, label, rows[row_idx], theme),
            8 => render_save_pw_toggle(frame, form, is_active, border_style, label, rows[row_idx], theme),
            9 => render_password_field(frame, form, is_active, border_style, rows[row_idx], theme),
            _ => {
                let value = match field_idx {
                    0 => &form.name,
                    1 => &form.host,
                    2 => &form.port,
                    3 => &form.user,
                    5 => &form.key_path,
                    6 => &form.remote_path,
                    7 => &form.local_start_path,
                    _ => "",
                };
                let value_style = if is_active {
                    Style::default().fg(theme.text_active).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_inactive)
                };
                let cursor = if is_active { "█" } else { "" };
                let field_title = if field_idx == 6 || field_idx == 7 {
                    format!(" {} (optional) ", label)
                } else {
                    format!(" {} ", label)
                };
                let field_block = Block::default()
                    .title(field_title)
                    .borders(Borders::ALL)
                    .border_style(border_style);
                let content = Line::from(vec![
                    Span::styled(value, value_style),
                    Span::styled(cursor, Style::default().fg(theme.cursor_bg)),
                ]);
                frame.render_widget(
                    Paragraph::new(content).block(field_block),
                    rows[row_idx],
                );
            }
        }
    }

    // Hint bar at bottom
    let hints = Line::from(vec![
        hint_key("Tab", theme), hint_label(" Nächstes Feld  ", theme),
        hint_key("Enter", theme), hint_label(" Speichern  ", theme),
        hint_key("Esc", theme), hint_label(" Abbrechen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), *rows.last().unwrap());
}

/// Render the Auth toggle field (key / password).
fn render_auth_toggle(
    frame: &mut Frame, form: &NewProfileForm,
    is_active: bool, border_style: Style, label: &str, area: Rect,
    theme: &Theme,
) {
    let (key_style, pw_style) = if form.auth == AuthMethod::Key {
        (
            Style::default().fg(theme.toggle_on).add_modifier(Modifier::BOLD),
            Style::default().fg(theme.toggle_off),
        )
    } else {
        (
            Style::default().fg(theme.toggle_off),
            Style::default().fg(theme.toggle_on).add_modifier(Modifier::BOLD),
        )
    };
    let hint = if is_active { "  [Space]" } else { "" };
    let field_block = Block::default()
        .title(format!(" {} ", label))
        .borders(Borders::ALL)
        .border_style(border_style);
    let auth_line = Line::from(vec![
        Span::styled("● key", key_style),
        Span::raw("   "),
        Span::styled("● password", pw_style),
        Span::styled(hint, Style::default().fg(theme.text_muted)),
    ]);
    frame.render_widget(Paragraph::new(auth_line).block(field_block), area);
}

/// Render the "Passwort speichern" toggle field (Ja / Nein).
fn render_save_pw_toggle(
    frame: &mut Frame, form: &NewProfileForm,
    is_active: bool, border_style: Style, label: &str, area: Rect,
    theme: &Theme,
) {
    let (ja_style, nein_style) = if form.save_password {
        (
            Style::default().fg(theme.toggle_on).add_modifier(Modifier::BOLD),
            Style::default().fg(theme.toggle_off),
        )
    } else {
        (
            Style::default().fg(theme.toggle_off),
            Style::default().fg(theme.toggle_on).add_modifier(Modifier::BOLD),
        )
    };
    let hint = if is_active { "  [Space]" } else { "" };
    let field_block = Block::default()
        .title(format!(" {} ", label))
        .borders(Borders::ALL)
        .border_style(border_style);
    let toggle_line = Line::from(vec![
        Span::styled("● Ja", ja_style),
        Span::raw("   "),
        Span::styled("● Nein", nein_style),
        Span::styled(hint, Style::default().fg(theme.text_muted)),
    ]);
    frame.render_widget(Paragraph::new(toggle_line).block(field_block), area);
}

/// Render the masked password input field.
fn render_password_field(
    frame: &mut Frame, form: &NewProfileForm,
    is_active: bool, border_style: Style, area: Rect,
    theme: &Theme,
) {
    let masked: String = "●".repeat(form.password.len());
    let value_style = if is_active {
        Style::default().fg(theme.text_active).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_inactive)
    };
    let cursor = if is_active { "█" } else { "" };
    let field_block = Block::default()
        .title(" Passwort ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let content = Line::from(vec![
        Span::styled(masked, value_style),
        Span::styled(cursor, Style::default().fg(theme.cursor_bg)),
    ]);
    frame.render_widget(Paragraph::new(content).block(field_block), area);
}

// ---------------------------------------------------------------------------
// Confirm delete
// ---------------------------------------------------------------------------

fn render_confirm_delete(
    frame: &mut Frame,
    dialog: &ProfileDialog,
    index: usize,
    area: Rect,
    theme: &Theme,
) {
    let confirm_area = centered_rect(50, 30, area);
    frame.render_widget(Clear, confirm_area);

    let name = dialog
        .store
        .profiles
        .get(index)
        .map(|p| p.name.as_str())
        .unwrap_or("?");

    let block = Block::default()
        .title(" Profil löschen? ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_error_border));

    let inner = block.inner(confirm_area);
    frame.render_widget(block, confirm_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let msg = Paragraph::new(Line::from(vec![
        Span::raw("Profil \""),
        Span::styled(
            name.to_string(),
            Style::default().fg(theme.text_warning).add_modifier(Modifier::BOLD),
        ),
        Span::raw("\" wirklich löschen?"),
    ]));
    frame.render_widget(msg, chunks[0]);

    let hints = Line::from(vec![
        hint_key("Enter / Y", theme), hint_label(" Ja  ", theme),
        hint_key("Esc / N", theme), hint_label(" Nein", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hint_key(k: &str, theme: &Theme) -> Span<'static> {
    Span::styled(
        format!(" {} ", k),
        Style::default()
            .bg(theme.badge_bg)
            .fg(theme.badge_fg)
            .add_modifier(Modifier::BOLD),
    )
}

fn hint_label(l: &str, theme: &Theme) -> Span<'static> {
    Span::styled(l.to_string(), Style::default().fg(theme.text_secondary))
}

/// Build a `Line` that shows the text with a block-cursor at `cursor_pos`.
/// Text before the cursor is primary, the cursor character (or a space if at
/// end) is shown with inverted cursor colours, text after is primary again.
fn cursor_line<'a>(input: &'a str, cursor_pos: usize, theme: &Theme) -> Line<'a> {
    let before = &input[..cursor_pos];

    // Find the end of the character sitting under the cursor (if any).
    let cursor_end = input[cursor_pos..]
        .char_indices()
        .nth(1)
        .map(|(i, _)| cursor_pos + i)
        .unwrap_or(input.len());

    let under = if cursor_pos < input.len() {
        &input[cursor_pos..cursor_end]
    } else {
        ""
    };
    let after = if cursor_end < input.len() {
        &input[cursor_end..]
    } else {
        ""
    };

    let cursor_span = if under.is_empty() {
        // Cursor is past the last character — show an empty block
        Span::styled(" ", Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg))
    } else {
        Span::styled(
            under.to_string(),
            Style::default()
                .bg(theme.cursor_bg)
                .fg(theme.cursor_fg)
                .add_modifier(Modifier::BOLD),
        )
    };

    Line::from(vec![
        Span::styled(before.to_string(), Style::default().fg(theme.text_primary)),
        cursor_span,
        Span::styled(after.to_string(), Style::default().fg(theme.text_primary)),
    ])
}

// ---------------------------------------------------------------------------
// Password dialog
// ---------------------------------------------------------------------------

/// Render the password prompt overlay.
pub fn render_password_dialog(frame: &mut Frame, dlg: &PasswordDialog, theme: &Theme) {
    let area = centered_rect(50, 40, frame.area());
    frame.render_widget(Clear, area);

    let title = format!(
        " Passwort für {}@{} ",
        dlg.profile.user, dlg.profile.host
    );
    let border_style = if dlg.error.is_some() {
        Style::default().fg(theme.dialog_error_border)
    } else {
        Style::default().fg(theme.dialog_warning_border)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // password input field
            Constraint::Length(1), // error line (or blank)
            Constraint::Min(0),
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Masked input
    let masked: String = "●".repeat(dlg.input.len());
    let cursor = "█";
    let input_block = Block::default()
        .title(" Passwort ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_active_border));
    let input_line = Line::from(vec![
        Span::styled(masked, Style::default().fg(theme.text_primary)),
        Span::styled(cursor, Style::default().fg(theme.cursor_bg)),
    ]);
    frame.render_widget(Paragraph::new(input_line).block(input_block), chunks[0]);

    // Error message
    if let Some(ref err) = dlg.error {
        let err_line = Line::from(Span::styled(
            format!("✗ {}", err),
            Style::default().fg(theme.text_danger),
        ));
        frame.render_widget(Paragraph::new(err_line), chunks[1]);
    }

    // Hints
    let hints = Line::from(vec![
        hint_key("Enter", theme), hint_label(" Verbinden  ", theme),
        hint_key("Esc", theme), hint_label(" Abbrechen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

// ---------------------------------------------------------------------------
// Rename dialog
// ---------------------------------------------------------------------------

/// Render the rename input dialog.
pub fn render_rename_dialog(frame: &mut Frame, dlg: &RenameDialog, theme: &Theme) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Umbenennen ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_warning_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // input field
            Constraint::Length(1), // hints
            Constraint::Min(0),
        ])
        .split(inner);

    let input_block = Block::default()
        .title(format!(" {} ", dlg.original))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_active_border));
    let input_line = cursor_line(&dlg.input, dlg.cursor_pos, theme);
    frame.render_widget(Paragraph::new(input_line).block(input_block), chunks[0]);

    let hints = Line::from(vec![
        hint_key("Enter", theme), hint_label(" OK  ", theme),
        hint_key("Esc", theme), hint_label(" Abbrechen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Mkdir dialog
// ---------------------------------------------------------------------------

/// Render the mkdir input dialog.
pub fn render_mkdir_dialog(frame: &mut Frame, dlg: &MkdirDialog, theme: &Theme) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Verzeichnis erstellen ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_warning_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // input field
            Constraint::Length(1), // hints
            Constraint::Min(0),
        ])
        .split(inner);

    let input_block = Block::default()
        .title(" Name ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_active_border));
    let input_line = cursor_line(&dlg.input, dlg.cursor_pos, theme);
    frame.render_widget(Paragraph::new(input_line).block(input_block), chunks[0]);

    let hints = Line::from(vec![
        hint_key("Enter", theme), hint_label(" Erstellen  ", theme),
        hint_key("Esc", theme), hint_label(" Abbrechen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Delete confirmation dialog
// ---------------------------------------------------------------------------

/// Render the delete confirmation dialog.
/// Shows a single entry name or a summary for multiple entries.
pub fn render_delete_dialog(frame: &mut Frame, dlg: &DeleteDialog, theme: &Theme) {
    let n = dlg.entries.len();

    // Height: 1 line per entry (max 6 shown) + 2 for padding/title + 1 hint
    let list_lines = n.min(6) as u16;
    let height_pct = (25 + list_lines * 3).min(80);
    let area = centered_rect(55, height_pct, frame.area());
    frame.render_widget(Clear, area);

    let location = match dlg.side {
        crate::app::PanelSide::Left => "Lokal",
        crate::app::PanelSide::Right => "Remote",
    };
    let title = if n == 1 {
        let (_name, is_dir) = &dlg.entries[0];
        let kind = if *is_dir { "Verzeichnis" } else { "Datei" };
        format!(" {} {} löschen? ", location, kind)
    } else {
        format!(" {} — {} Einträge löschen? ", location, n)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_error_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // entry list
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Build the list of entries to show (cap at 6, add "… and N more" if needed)
    let mut items: Vec<ListItem> = dlg
        .entries
        .iter()
        .take(6)
        .map(|(name, is_dir)| {
            let icon = if *is_dir { "▶ " } else { "  " };
            let icon_style = if *is_dir {
                Style::default().fg(theme.directory_icon).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.file_name)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {}", icon), icon_style),
                Span::styled(name.clone(), Style::default().fg(theme.text_primary).add_modifier(Modifier::BOLD)),
            ]))
        })
        .collect();

    if n > 6 {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  … und {} weitere", n - 6),
            Style::default().fg(theme.text_muted),
        ))));
    }

    frame.render_widget(List::new(items), chunks[0]);

    let hints = Line::from(vec![
        hint_key("Y/Enter", theme), hint_label(" Löschen  ", theme),
        hint_key("N/Esc", theme), hint_label(" Abbrechen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Help / keyboard shortcut overlay (F1)
// ---------------------------------------------------------------------------

/// All shortcuts shown in the help overlay.
/// Each entry is (key_label, description).
const SHORTCUTS: &[(&str, &str)] = &[
    // Navigation
    ("↑ / ↓",         "Cursor bewegen"),
    ("Enter",          "Verzeichnis öffnen / Datei bearbeiten"),
    ("Backspace",      "Übergeordnetes Verzeichnis"),
    ("Tab",            "Panel wechseln (lokal ↔ remote)"),
    ("Ctrl+U / Ctrl+S","Panels tauschen (lokal ↔ remote, nur visuell)"),
    ("Ctrl+T",          "Theme umschalten (Auto/Dark/Light)"),
    // Selection
    ("Leertaste",      "Datei/Verzeichnis markieren"),
    ("*",              "Alle markieren / alle abwählen"),
    // File operations
    ("F2",             "Umbenennen"),
    ("F4",             "Datei bearbeiten (lokal: $EDITOR / remote: dl→edit→ul)"),
    ("F5",             "Upload (lokal → remote)"),
    ("F6",             "Download (remote → lokal)"),
    ("F7",             "Verzeichnis erstellen"),
    ("F8",             "Löschen (mit Bestätigung)"),
    ("!",              "Shell-Befehl im lokalen Verzeichnis ausführen"),
    // Connection
    ("F3",             "Verbindung trennen"),
    ("F9  /  p",       "Verbindungsprofile öffnen"),
    ("E  /  F2",       "Profil bearbeiten (im Profil-Dialog)"),
    // App
    ("F1",             "Diese Hilfe anzeigen / schließen"),
    ("F10  /  q",      "Beenden"),
];

pub fn render_help_dialog(frame: &mut Frame, theme: &Theme) {
    let area = centered_rect(60, 85, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Tastaturkürzel — F1 / Esc zum Schließen ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_active_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner: shortcut list + bottom hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let key_col_w = 16usize;

    let items: Vec<ListItem> = SHORTCUTS
        .iter()
        .map(|(key, desc)| {
            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<width$}", key, width = key_col_w),
                    Style::default()
                        .fg(theme.dialog_active_border)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", desc),
                    Style::default().fg(theme.text_primary),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[0]);

    let close_hint = Line::from(vec![
        hint_key("F1", theme), hint_label(" / ", theme),
        hint_key("Esc", theme), hint_label(" Schließen", theme),
    ]);
    frame.render_widget(Paragraph::new(close_hint), chunks[1]);
}

// ---------------------------------------------------------------------------
// Shell command dialog ('!')
// ---------------------------------------------------------------------------

pub fn render_shell_dialog(frame: &mut Frame, dlg: &ShellDialog, cwd: &std::path::Path, theme: &Theme) {
    if dlg.output.is_none() {
        render_shell_input(frame, dlg, cwd, theme);
    } else {
        render_shell_output(frame, dlg, theme);
    }
}

fn render_shell_input(frame: &mut Frame, dlg: &ShellDialog, cwd: &std::path::Path, theme: &Theme) {
    let area = centered_rect(70, 25, frame.area());
    frame.render_widget(Clear, area);

    let cwd_str = cwd.to_string_lossy();
    let title = format!(" Shell  {}  ", cwd_str);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_warning_border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(1), // input
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hints
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " Befehl:",
            Style::default().fg(theme.shell_label).add_modifier(Modifier::BOLD),
        ))),
        chunks[0],
    );

    // Build input line with cursor block.
    let before: &str = &dlg.input[..dlg.cursor_pos];
    let cursor_char = dlg.input[dlg.cursor_pos..]
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_else(|| " ".to_string());
    let after: &str = if dlg.cursor_pos < dlg.input.len() {
        let end = dlg.cursor_pos + cursor_char.len();
        &dlg.input[end..]
    } else {
        ""
    };
    let input_line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(before, Style::default().fg(theme.text_primary)),
        Span::styled(
            cursor_char,
            Style::default().bg(theme.shell_cursor_bg).fg(theme.shell_cursor_fg),
        ),
        Span::styled(after, Style::default().fg(theme.text_primary)),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[1]);

    let hints = Line::from(vec![
        hint_key("Enter", theme), hint_label(" Ausführen  ", theme),
        hint_key("Esc", theme), hint_label(" Abbrechen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

fn render_shell_output(frame: &mut Frame, dlg: &ShellDialog, theme: &Theme) {
    let area = centered_rect(85, 75, frame.area());
    frame.render_widget(Clear, area);

    let code_str = dlg.exit_code
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".to_string());
    let title = format!(" Ausgabe  Exit: {}  ", code_str);
    let exit_color = match dlg.exit_code {
        Some(0) => theme.dialog_success_border,
        Some(_) => theme.dialog_error_border,
        None    => theme.dialog_warning_border,
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(exit_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    // Build output text — join lines, use Paragraph scroll.
    let lines: Vec<Line> = dlg
        .output
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(theme.text_primary))))
        .collect();

    let output_para = Paragraph::new(lines)
        .style(Style::default().bg(theme.shell_output_bg))
        .scroll((dlg.scroll as u16, 0));
    frame.render_widget(output_para, chunks[0]);

    let hints = Line::from(vec![
        hint_key("↑↓", theme), hint_label(" Scrollen  ", theme),
        hint_key("PgUp/PgDn", theme), hint_label(" Seite  ", theme),
        hint_key("Esc", theme), hint_label(" Schließen", theme),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

/// Return a Rect centered within `r` with the given percentage dimensions.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render the permission fix dialog overlay.
pub fn render_permission_dialog(frame: &mut Frame, dlg: &PermissionFixDialog, theme: &Theme) {
    let message_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                "⚠   Warnung: Unsichere Berechtigungen!   ⚠",
                Style::default().fg(theme.text_warning).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Datei: "),
            Span::styled(&dlg.path, Style::default().fg(theme.text_info)),
        ]),
        Line::from(vec![
            Span::raw("Aktuelle Rechte: "),
            Span::styled(format!("{:04o}", dlg.mode), Style::default().fg(theme.text_danger)),
        ]),
        Line::from(vec![
            Span::raw("Erforderlich: "),
            Span::styled("0600", Style::default().fg(theme.text_success)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Andere Benutzer können die Datei "),
            Span::styled("lesen", Style::default().fg(theme.text_danger).add_modifier(Modifier::BOLD)),
            Span::raw(" oder "),
            Span::styled("schreiben", Style::default().fg(theme.text_danger).add_modifier(Modifier::BOLD)),
            Span::raw("."),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Drücke "),
            Span::styled("f", Style::default().fg(theme.text_success).add_modifier(Modifier::BOLD)),
            Span::raw(" um Rechte auf 0600 zu setzen"),
        ]),
        Line::from(vec![
            Span::raw("Drücke "),
            Span::styled("i", Style::default().fg(theme.text_info).add_modifier(Modifier::BOLD)),
            Span::raw(" oder "),
            Span::styled("Esc", Style::default().fg(theme.text_info).add_modifier(Modifier::BOLD)),
            Span::raw(" um fortzufahren"),
        ]),
    ];

    let block = Block::default()
        .title(" Berechtigungen ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_warning_border));

    let para = Paragraph::new(Text::from(message_lines))
        .wrap(Wrap { trim: true })
        .block(block)
        .alignment(Alignment::Left);

    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Unknown host key dialog
// ---------------------------------------------------------------------------

pub fn render_host_key_dialog(frame: &mut Frame, dlg: &HostKeyDialog, theme: &Theme) {
    let message_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                "⚠   Unbekannter Host-Key!   ⚠",
                Style::default().fg(theme.text_warning).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Host:        "),
            Span::styled(
                format!("{}:{}", dlg.host, dlg.port),
                Style::default().fg(theme.dialog_active_border),
            ),
        ]),
        Line::from(vec![
            Span::raw("Key-Typ:     "),
            Span::styled(&dlg.key_type, Style::default().fg(theme.text_secondary)),
        ]),
        Line::from(vec![
            Span::raw("Fingerprint: "),
            Span::styled(&dlg.fingerprint, Style::default().fg(theme.text_primary).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Dieser Host ist "),
            Span::styled("nicht", Style::default().fg(theme.text_danger).add_modifier(Modifier::BOLD)),
            Span::raw(" in ~/.ssh/known_hosts vorhanden."),
        ]),
        Line::from(vec![
            Span::raw("Bitte prüfe den Fingerprint aus einer vertrauenswürdigen Quelle."),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Y / Enter", Style::default().fg(theme.text_success).add_modifier(Modifier::BOLD)),
            Span::raw(" — Vertrauen und zu known_hosts hinzufügen"),
        ]),
        Line::from(vec![
            Span::styled("N / Esc", Style::default().fg(theme.text_danger).add_modifier(Modifier::BOLD)),
            Span::raw("   — Verbindung abbrechen"),
        ]),
    ];

    let block = Block::default()
        .title(" Unbekannter Host-Key ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog_warning_border));

    let para = Paragraph::new(Text::from(message_lines))
        .wrap(Wrap { trim: false })
        .block(block)
        .alignment(Alignment::Left);

    let area = centered_rect(65, 55, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(para, area);
}
