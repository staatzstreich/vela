use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{
    DeleteDialog, MkdirDialog, NewProfileForm, PasswordDialog, ProfileDialog, ProfileDialogMode,
    RenameDialog, ShellDialog,
};
use crate::config::profiles::AuthMethod;

/// Render the profile manager dialog centered on the screen.
pub fn render_profile_dialog(frame: &mut Frame, dialog: &ProfileDialog) {
    let area = centered_rect(70, 80, frame.area());
    frame.render_widget(Clear, area);

    match &dialog.mode {
        ProfileDialogMode::List => render_list(frame, dialog, area),
        ProfileDialogMode::New { field } => {
            render_profile_form(frame, &dialog.form, *field, area, " Neues Profil ")
        }
        ProfileDialogMode::Edit { field, .. } => {
            render_profile_form(frame, &dialog.form, *field, area, " Profil bearbeiten ")
        }
        ProfileDialogMode::ConfirmDelete { index } => {
            render_confirm_delete(frame, dialog, *index, area)
        }
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn render_list(frame: &mut Frame, dialog: &ProfileDialog, area: Rect) {
    let block = Block::default()
        .title(" Verbindungsprofile (F9) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

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
            Style::default().fg(Color::DarkGray),
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
                    Span::styled(active_marker, Style::default().fg(Color::Green)),
                    Span::styled(
                        format!("{:<20}", p.name),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}@{}:{}", p.user, p.host, p.port),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(
                        format!("  [{}]", p.auth.as_str()),
                        Style::default().fg(Color::DarkGray),
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
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("► ");

    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    // Hint bar
    let hints = Line::from(vec![
        hint_key("Enter"), hint_label(" Auswählen  "),
        hint_key("N"), hint_label(" Neu  "),
        hint_key("E / F2"), hint_label(" Bearbeiten  "),
        hint_key("D"), hint_label(" Löschen  "),
        hint_key("Esc"), hint_label(" Schließen"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// New-profile form
// ---------------------------------------------------------------------------

/// Field indices: 0=Name 1=Host 2=Port 3=User 4=Auth(toggle) 5=KeyPath 6=RemotePath 7=LocalPath
const FIELD_LABELS: &[&str] = &["Name", "Host", "Port", "User", "Auth", "Key-Pfad", "Remote-Startpfad", "Lokaler Startpfad"];
const FIELD_COUNT: usize = 8;

fn render_profile_form(frame: &mut Frame, form: &NewProfileForm, active_field: usize, area: Rect, title: &str) {
    let block = Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut constraints: Vec<Constraint> = (0..FIELD_COUNT)
        .map(|_| Constraint::Length(3))
        .collect();
    constraints.push(Constraint::Min(0));
    constraints.push(Constraint::Length(1));

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let field_values: [&str; FIELD_COUNT] = [
        &form.name,
        &form.host,
        &form.port,
        &form.user,
        form.auth.as_str(),
        &form.key_path,
        &form.remote_path,
        &form.local_start_path,
    ];

    for (i, label) in FIELD_LABELS.iter().enumerate() {
        let is_active = i == active_field;
        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let value_style = if is_active {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Auth field: toggle display
        if i == 4 {
            let (key_style, pw_style) = if form.auth == AuthMethod::Key {
                (
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::DarkGray),
                )
            } else {
                (
                    Style::default().fg(Color::DarkGray),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )
            };
            let hint = if is_active { "  [Space zum Wechseln]" } else { "" };
            let field_block = Block::default()
                .title(format!(" {} ", label))
                .borders(Borders::ALL)
                .border_style(border_style);
            let auth_line = Line::from(vec![
                Span::styled("● key", key_style),
                Span::raw("   "),
                Span::styled("● password", pw_style),
                Span::styled(hint, Style::default().fg(Color::DarkGray)),
            ]);
            frame.render_widget(
                Paragraph::new(auth_line).block(field_block),
                rows[i],
            );
            continue;
        }

        let cursor = if is_active { "█" } else { "" };
        // For the RemotePath and LocalPath fields show an "(optional)" hint in the title.
        let field_title = if i == 6 || i == 7 {
            format!(" {} (optional) ", label)
        } else {
            format!(" {} ", label)
        };
        let field_block = Block::default()
            .title(field_title)
            .borders(Borders::ALL)
            .border_style(border_style);
        let content = Line::from(vec![
            Span::styled(field_values[i], value_style),
            Span::styled(cursor, Style::default().fg(Color::Cyan)),
        ]);
        frame.render_widget(
            Paragraph::new(content).block(field_block),
            rows[i],
        );
    }

    // Hint bar at bottom
    let hints = Line::from(vec![
        hint_key("Tab"), hint_label(" Nächstes Feld  "),
        hint_key("Enter"), hint_label(" Speichern  "),
        hint_key("Esc"), hint_label(" Abbrechen"),
    ]);
    frame.render_widget(Paragraph::new(hints), *rows.last().unwrap());
}

// ---------------------------------------------------------------------------
// Confirm delete
// ---------------------------------------------------------------------------

fn render_confirm_delete(
    frame: &mut Frame,
    dialog: &ProfileDialog,
    index: usize,
    area: Rect,
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
        .border_style(Style::default().fg(Color::Red));

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
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("\" wirklich löschen?"),
    ]));
    frame.render_widget(msg, chunks[0]);

    let hints = Line::from(vec![
        hint_key("Enter / Y"), hint_label(" Ja  "),
        hint_key("Esc / N"), hint_label(" Nein"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hint_key(k: &str) -> Span<'static> {
    Span::styled(
        format!(" {} ", k),
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
}

fn hint_label(l: &str) -> Span<'static> {
    Span::styled(l.to_string(), Style::default().fg(Color::Gray))
}

/// Build a `Line` that shows the text with a block-cursor at `cursor_pos`.
/// Text before the cursor is white, the cursor character (or a space if at
/// end) is shown with inverted Cyan colours, text after is white again.
fn cursor_line<'a>(input: &'a str, cursor_pos: usize) -> Line<'a> {
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
        Span::styled(" ", Style::default().bg(Color::Cyan).fg(Color::Black))
    } else {
        Span::styled(
            under.to_string(),
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
    };

    Line::from(vec![
        Span::styled(before.to_string(), Style::default().fg(Color::White)),
        cursor_span,
        Span::styled(after.to_string(), Style::default().fg(Color::White)),
    ])
}

// ---------------------------------------------------------------------------
// Password dialog
// ---------------------------------------------------------------------------

/// Render the password prompt overlay.
pub fn render_password_dialog(frame: &mut Frame, dlg: &PasswordDialog) {
    let area = centered_rect(50, 40, frame.area());
    frame.render_widget(Clear, area);

    let title = format!(
        " Passwort für {}@{} ",
        dlg.profile.user, dlg.profile.host
    );
    let border_style = if dlg.error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Yellow)
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
        .border_style(Style::default().fg(Color::Cyan));
    let input_line = Line::from(vec![
        Span::styled(masked, Style::default().fg(Color::White)),
        Span::styled(cursor, Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(input_line).block(input_block), chunks[0]);

    // Error message
    if let Some(ref err) = dlg.error {
        let err_line = Line::from(Span::styled(
            format!("✗ {}", err),
            Style::default().fg(Color::Red),
        ));
        frame.render_widget(Paragraph::new(err_line), chunks[1]);
    }

    // Hints
    let hints = Line::from(vec![
        hint_key("Enter"), hint_label(" Verbinden  "),
        hint_key("Esc"), hint_label(" Abbrechen"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

// ---------------------------------------------------------------------------
// Rename dialog
// ---------------------------------------------------------------------------

/// Render the rename input dialog.
pub fn render_rename_dialog(frame: &mut Frame, dlg: &RenameDialog) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Umbenennen ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

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
        .border_style(Style::default().fg(Color::Cyan));
    let input_line = cursor_line(&dlg.input, dlg.cursor_pos);
    frame.render_widget(Paragraph::new(input_line).block(input_block), chunks[0]);

    let hints = Line::from(vec![
        hint_key("Enter"), hint_label(" OK  "),
        hint_key("Esc"), hint_label(" Abbrechen"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Mkdir dialog
// ---------------------------------------------------------------------------

/// Render the mkdir input dialog.
pub fn render_mkdir_dialog(frame: &mut Frame, dlg: &MkdirDialog) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Verzeichnis erstellen ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

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
        .border_style(Style::default().fg(Color::Cyan));
    let input_line = cursor_line(&dlg.input, dlg.cursor_pos);
    frame.render_widget(Paragraph::new(input_line).block(input_block), chunks[0]);

    let hints = Line::from(vec![
        hint_key("Enter"), hint_label(" Erstellen  "),
        hint_key("Esc"), hint_label(" Abbrechen"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

// ---------------------------------------------------------------------------
// Delete confirmation dialog
// ---------------------------------------------------------------------------

/// Render the delete confirmation dialog.
/// Shows a single entry name or a summary for multiple entries.
pub fn render_delete_dialog(frame: &mut Frame, dlg: &DeleteDialog) {
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
        .border_style(Style::default().fg(Color::Red));

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
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {}", icon), icon_style),
                Span::styled(name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ]))
        })
        .collect();

    if n > 6 {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  … und {} weitere", n - 6),
            Style::default().fg(Color::DarkGray),
        ))));
    }

    frame.render_widget(List::new(items), chunks[0]);

    let hints = Line::from(vec![
        hint_key("Y/Enter"), hint_label(" Löschen  "),
        hint_key("N/Esc"), hint_label(" Abbrechen"),
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

pub fn render_help_dialog(frame: &mut Frame) {
    let area = centered_rect(60, 85, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Tastaturkürzel — F1 / Esc zum Schließen ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

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
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", desc),
                    Style::default().fg(Color::White),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[0]);

    let close_hint = Line::from(vec![
        hint_key("F1"), hint_label(" / "),
        hint_key("Esc"), hint_label(" Schließen"),
    ]);
    frame.render_widget(Paragraph::new(close_hint), chunks[1]);
}

// ---------------------------------------------------------------------------
// Shell command dialog ('!')
// ---------------------------------------------------------------------------

pub fn render_shell_dialog(frame: &mut Frame, dlg: &ShellDialog, cwd: &std::path::Path) {
    if dlg.output.is_none() {
        render_shell_input(frame, dlg, cwd);
    } else {
        render_shell_output(frame, dlg);
    }
}

fn render_shell_input(frame: &mut Frame, dlg: &ShellDialog, cwd: &std::path::Path) {
    let area = centered_rect(70, 25, frame.area());
    frame.render_widget(Clear, area);

    let cwd_str = cwd.to_string_lossy();
    let title = format!(" Shell  {}  ", cwd_str);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

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
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled(
            cursor_char,
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::styled(after, Style::default().fg(Color::White)),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[1]);

    let hints = Line::from(vec![
        hint_key("Enter"), hint_label(" Ausführen  "),
        hint_key("Esc"), hint_label(" Abbrechen"),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

fn render_shell_output(frame: &mut Frame, dlg: &ShellDialog) {
    let area = centered_rect(85, 75, frame.area());
    frame.render_widget(Clear, area);

    let code_str = dlg.exit_code
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".to_string());
    let title = format!(" Ausgabe  Exit: {}  ", code_str);
    let exit_color = match dlg.exit_code {
        Some(0) => Color::Green,
        Some(_) => Color::Red,
        None    => Color::Yellow,
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
        .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::White))))
        .collect();

    let output_para = Paragraph::new(lines)
        .style(Style::default().bg(Color::Black))
        .scroll((dlg.scroll as u16, 0));
    frame.render_widget(output_para, chunks[0]);

    let hints = Line::from(vec![
        hint_key("↑↓"), hint_label(" Scrollen  "),
        hint_key("PgUp/PgDn"), hint_label(" Seite  "),
        hint_key("Esc"), hint_label(" Schließen"),
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
