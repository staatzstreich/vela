mod app;
mod config;
mod connection;
mod transfer;
mod ui;

use std::io;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::{ActivePanel, App, AppError, EditRequest, ProfileDialogMode};
use config::profiles::AuthMethod;

fn main() -> Result<(), AppError> {
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, AppError> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), AppError> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), AppError> {
    let mut app = App::new()?;

    while app.running {
        // Poll transfer state before rendering so the UI reflects completion immediately
        app.poll_upload();
        app.poll_download();
        terminal.draw(|frame| ui::render(frame, &app))?;
        handle_events(&mut app)?;

        // F4: if an editor launch was requested, hand off to the editor and
        // restore the TUI afterwards.  launch_editor() owns the full
        // suspend/restore cycle via ratatui::restore() / ratatui::init().
        if let Some(req) = app.pending_edit.take() {
            launch_editor(&req);
            terminal.clear()?;
            app.finish_edit(req)?;
        }
    }

    Ok(())
}

/// Find an editor binary that is actually installed on this system.
/// Search order: $EDITOR, $VISUAL, vim, nano, vi.
/// Each candidate is verified with `which` before being accepted.
/// Returns None only when none of the candidates exist.
fn find_editor() -> Option<String> {
    let candidates: Vec<String> = [
        std::env::var("EDITOR").ok(),
        std::env::var("VISUAL").ok(),
        Some("vim".to_string()),
        Some("nano".to_string()),
        Some("vi".to_string()),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.trim().is_empty())
    .collect();

    for candidate in candidates {
        // `which` exits 0 when the binary is found on PATH.
        let found = std::process::Command::new("which")
            .arg(&candidate)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if found {
            return Some(candidate);
        }
    }
    None
}

/// Suspend the TUI, launch the editor for `req`, then restore the TUI.
/// Uses `ratatui::restore()` / `ratatui::init()` for clean terminal handover.
/// Ignores the editor exit code — mtime comparison determines whether a file
/// was saved.
fn launch_editor(req: &EditRequest) {
    let path = match req {
        EditRequest::Local  { path }            => path,
        EditRequest::Remote { temp_path, .. }   => temp_path,
    };
    match find_editor() {
        Some(editor) => {
            ratatui::restore();
            let _ = std::process::Command::new(&editor).arg(path).status();
            ratatui::init();
        }
        None => {
            // No editor found — nothing to do; finish_edit will see no mtime change.
        }
    }
}

fn handle_events(app: &mut App) -> Result<(), AppError> {
    if !event::poll(std::time::Duration::from_millis(50))? {
        return Ok(());
    }

    if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // F1 toggles the help overlay from any context.
        // Esc closes it when it is visible.
        if key.code == KeyCode::F(1) {
            app.help_visible = !app.help_visible;
            return Ok(());
        }
        if app.help_visible {
            if key.code == KeyCode::Esc {
                app.help_visible = false;
            }
            return Ok(());
        }

        // Ctrl+U / Ctrl+S — swap panels visually (works from any mode)
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('u') | KeyCode::Char('s'))
        {
            app.swap_panels();
            return Ok(());
        }

        // Priority (highest first): password > delete > rename > mkdir > shell > profile > main
        if app.password_dialog.is_some() {
            handle_password_key(app, key.code);
        } else if app.delete_dialog.is_some() {
            handle_delete_key(app, key.code);
        } else if app.rename_dialog.is_some() {
            handle_rename_key(app, key.code);
        } else if app.mkdir_dialog.is_some() {
            handle_mkdir_key(app, key.code);
        } else if app.shell_dialog.is_some() {
            handle_shell_key(app, key.code);
        } else if app.profile_dialog.is_some() {
            handle_dialog_key(app, key.code);
        } else {
            handle_main_key(app, key.code)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main panel key handling
// ---------------------------------------------------------------------------

fn handle_main_key(app: &mut App, code: KeyCode) -> Result<(), AppError> {
    match code {
        KeyCode::F(10) | KeyCode::Char('q') => app.quit(),
        KeyCode::Tab => app.toggle_panel(),
        KeyCode::Up => app.active_panel_mut().move_up(),
        KeyCode::Down => app.active_panel_mut().move_down(),

        // Space = toggle mark on current entry; move down after marking
        KeyCode::Char(' ') => {
            app.active_panel_mut().toggle_mark();
            app.active_panel_mut().move_down();
        }

        // * = mark all / unmark all in active panel
        KeyCode::Char('*') => {
            app.active_panel_mut().mark_all();
        }

        KeyCode::Enter => match app.active {
            ActivePanel::Left => {
                if let Err(e) = app.left.enter_selected() {
                    app.status_message = Some(e.to_string());
                }
            }
            ActivePanel::Right => {
                if app.is_connected() {
                    app.remote_enter_selected();
                }
            }
        },

        KeyCode::Backspace => match app.active {
            ActivePanel::Left => {
                if let Err(e) = app.left.go_up() {
                    app.status_message = Some(e.to_string());
                }
            }
            ActivePanel::Right => {
                if app.is_connected() {
                    app.remote_go_up();
                }
            }
        },

        // F3 = disconnect (only when connected)
        KeyCode::F(3) => {
            if app.is_connected() {
                app.disconnect();
            }
        }

        // F5 = upload (left panel → remote)
        KeyCode::F(5) => {
            if app.is_connected() && !app.is_transferring() {
                app.start_upload();
            }
        }

        // F6 = download (remote → left panel)
        KeyCode::F(6) => {
            if app.is_connected() && !app.is_transferring() {
                app.start_download();
            }
        }

        // F2 = rename selected entry
        KeyCode::F(2) => app.open_rename_dialog(),

        // F4 = edit selected file in $EDITOR
        KeyCode::F(4) => app.prepare_edit(),

        // F7 = create new directory
        KeyCode::F(7) => app.open_mkdir_dialog(),

        // F8 = delete selected entry
        KeyCode::F(8) => app.open_delete_dialog(),

        // ! = shell command dialog
        KeyCode::Char('!') => app.open_shell_dialog(),

        // F9 / p = profile manager
        KeyCode::F(9) | KeyCode::Char('p') => app.open_profile_dialog(),

        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Profile dialog key handling
// ---------------------------------------------------------------------------

fn handle_dialog_key(app: &mut App, code: KeyCode) {
    let mode = match app.profile_dialog.as_ref() {
        Some(d) => d.mode.clone(),
        None => return,
    };

    match mode {
        ProfileDialogMode::List => handle_list_key(app, code),
        ProfileDialogMode::New { field } => handle_new_form_key(app, code, field),
        ProfileDialogMode::Edit { field, index } => handle_edit_form_key(app, code, field, index),
        ProfileDialogMode::ConfirmDelete { index } => handle_confirm_delete_key(app, code, index),
    }
}

fn handle_list_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.close_profile_dialog(),
        KeyCode::Up => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.list_move_up();
            }
        }
        KeyCode::Down => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.list_move_down();
            }
        }
        KeyCode::Enter => {
            // Take the selected profile and initiate connection
            if let Some(d) = app.profile_dialog.as_ref() {
                if d.store.profiles.is_empty() {
                    return;
                }
                let profile = d.store.profiles[d.list_selected].clone();
                app.close_profile_dialog();
                app.begin_connect(profile);
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.mode = ProfileDialogMode::New { field: 0 };
                d.form = crate::app::NewProfileForm::new();
            }
        }
        KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::F(2) => {
            if let Some(d) = app.profile_dialog.as_mut() {
                if !d.store.profiles.is_empty() {
                    let idx = d.list_selected;
                    let p = &d.store.profiles[idx];
                    d.form = crate::app::NewProfileForm {
                        name:             p.name.clone(),
                        host:             p.host.clone(),
                        port:             p.port.to_string(),
                        user:             p.user.clone(),
                        auth:             p.auth.clone(),
                        key_path:         p.key_path.clone().unwrap_or_else(|| "~/.ssh/id_rsa".to_string()),
                        remote_path:      p.remote_path.clone().unwrap_or_default(),
                        local_start_path: p.local_start_path.clone().unwrap_or_default(),
                    };
                    d.mode = ProfileDialogMode::Edit { field: 0, index: idx };
                }
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
            if let Some(d) = app.profile_dialog.as_mut() {
                if !d.store.profiles.is_empty() {
                    let idx = d.list_selected;
                    d.mode = ProfileDialogMode::ConfirmDelete { index: idx };
                }
            }
        }
        _ => {}
    }
}

fn handle_new_form_key(app: &mut App, code: KeyCode, field: usize) {
    match code {
        KeyCode::Esc => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.mode = ProfileDialogMode::List;
            }
        }
        KeyCode::Tab => {
            if let Some(d) = app.profile_dialog.as_mut() {
                let next = next_field(field, &d.form.auth);
                d.mode = ProfileDialogMode::New { field: next };
            }
        }
        KeyCode::BackTab => {
            if let Some(d) = app.profile_dialog.as_mut() {
                let prev = prev_field(field, &d.form.auth);
                d.mode = ProfileDialogMode::New { field: prev };
            }
        }
        KeyCode::Char(' ') if field == 4 => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.form.auth = match d.form.auth {
                    AuthMethod::Key => AuthMethod::Password,
                    AuthMethod::Password => AuthMethod::Key,
                };
            }
        }
        KeyCode::Enter => {
            if let Some(d) = app.profile_dialog.as_mut() {
                match d.form.to_profile() {
                    Some(profile) => {
                        let name = profile.name.clone();
                        d.store.add(profile);
                        match d.save() {
                            Ok(()) => {
                                app.status_message =
                                    Some(format!("Profil '{}' gespeichert", name));
                            }
                            Err(e) => {
                                app.status_message =
                                    Some(format!("Speichern fehlgeschlagen: {}", e));
                            }
                        }
                        if let Some(d) = app.profile_dialog.as_mut() {
                            d.mode = ProfileDialogMode::List;
                        }
                    }
                    None => {
                        app.status_message =
                            Some("Name, Host und User dürfen nicht leer sein".to_string());
                    }
                }
            }
        }
        KeyCode::Backspace => {
            if let Some(d) = app.profile_dialog.as_mut() {
                if let Some(s) = d.form.active_field_mut(field) {
                    s.pop();
                }
            }
        }
        KeyCode::Char(c) if field != 4 => {
            if let Some(d) = app.profile_dialog.as_mut() {
                if field == 2 && !c.is_ascii_digit() {
                    return;
                }
                if let Some(s) = d.form.active_field_mut(field) {
                    s.push(c);
                }
            }
        }
        _ => {}
    }
}

fn handle_edit_form_key(app: &mut App, code: KeyCode, field: usize, index: usize) {
    match code {
        KeyCode::Esc => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.mode = ProfileDialogMode::List;
            }
        }
        KeyCode::Tab => {
            if let Some(d) = app.profile_dialog.as_mut() {
                let next = next_field(field, &d.form.auth);
                d.mode = ProfileDialogMode::Edit { field: next, index };
            }
        }
        KeyCode::BackTab => {
            if let Some(d) = app.profile_dialog.as_mut() {
                let prev = prev_field(field, &d.form.auth);
                d.mode = ProfileDialogMode::Edit { field: prev, index };
            }
        }
        KeyCode::Char(' ') if field == 4 => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.form.auth = match d.form.auth {
                    AuthMethod::Key => AuthMethod::Password,
                    AuthMethod::Password => AuthMethod::Key,
                };
            }
        }
        KeyCode::Enter => {
            if let Some(d) = app.profile_dialog.as_mut() {
                match d.form.to_profile() {
                    Some(profile) => {
                        let name = profile.name.clone();
                        d.store.update(index, profile);
                        match d.save() {
                            Ok(()) => {
                                app.status_message =
                                    Some(format!("Profil '{}' aktualisiert", name));
                            }
                            Err(e) => {
                                app.status_message =
                                    Some(format!("Speichern fehlgeschlagen: {}", e));
                            }
                        }
                        if let Some(d) = app.profile_dialog.as_mut() {
                            d.mode = ProfileDialogMode::List;
                        }
                    }
                    None => {
                        app.status_message =
                            Some("Name, Host und User dürfen nicht leer sein".to_string());
                    }
                }
            }
        }
        KeyCode::Backspace => {
            if let Some(d) = app.profile_dialog.as_mut() {
                if let Some(s) = d.form.active_field_mut(field) {
                    s.pop();
                }
            }
        }
        KeyCode::Char(c) if field != 4 => {
            if let Some(d) = app.profile_dialog.as_mut() {
                if field == 2 && !c.is_ascii_digit() {
                    return;
                }
                if let Some(s) = d.form.active_field_mut(field) {
                    s.push(c);
                }
            }
        }
        _ => {}
    }
}

fn handle_confirm_delete_key(app: &mut App, code: KeyCode, index: usize) {
    match code {
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.store.remove(index);
                let len = d.store.profiles.len();
                if d.list_selected >= len && len > 0 {
                    d.list_selected = len - 1;
                }
                match d.save() {
                    Ok(()) => app.status_message = Some("Profil gelöscht".to_string()),
                    Err(e) => {
                        app.status_message = Some(format!("Löschen fehlgeschlagen: {}", e));
                    }
                }
                if let Some(d) = app.profile_dialog.as_mut() {
                    d.mode = ProfileDialogMode::List;
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            if let Some(d) = app.profile_dialog.as_mut() {
                d.mode = ProfileDialogMode::List;
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Password dialog key handling
// ---------------------------------------------------------------------------

fn handle_password_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.password_dialog = None;
            app.status_message = Some("Verbindung abgebrochen".to_string());
        }
        KeyCode::Enter => {
            // Take the dialog out, attempt connect, put back on failure
            if let Some(dlg) = app.password_dialog.take() {
                let password = dlg.input.clone();
                let profile = dlg.profile.clone();
                app.password_dialog = Some(dlg); // restore so error can be written
                app.do_connect(profile, Some(&password));
            }
        }
        KeyCode::Backspace => {
            if let Some(dlg) = app.password_dialog.as_mut() {
                dlg.input.pop();
                dlg.error = None;
            }
        }
        KeyCode::Char(c) => {
            if let Some(dlg) = app.password_dialog.as_mut() {
                dlg.input.push(c);
                dlg.error = None;
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Rename dialog key handling
// ---------------------------------------------------------------------------

fn handle_rename_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.rename_dialog = None;
        }
        KeyCode::Enter => {
            app.confirm_rename();
        }
        KeyCode::Left => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.move_left();
            }
        }
        KeyCode::Right => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.move_right();
            }
        }
        KeyCode::Home => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.move_home();
            }
        }
        KeyCode::End => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.move_end();
            }
        }
        KeyCode::Backspace => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.backspace();
            }
        }
        KeyCode::Delete => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.delete_forward();
            }
        }
        KeyCode::Char(c) => {
            if let Some(dlg) = app.rename_dialog.as_mut() {
                dlg.insert(c);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Mkdir dialog key handling
// ---------------------------------------------------------------------------

fn handle_mkdir_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.mkdir_dialog = None;
        }
        KeyCode::Enter => {
            app.confirm_mkdir();
        }
        KeyCode::Left => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.move_left();
            }
        }
        KeyCode::Right => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.move_right();
            }
        }
        KeyCode::Home => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.move_home();
            }
        }
        KeyCode::End => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.move_end();
            }
        }
        KeyCode::Backspace => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.backspace();
            }
        }
        KeyCode::Delete => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.delete_forward();
            }
        }
        KeyCode::Char(c) => {
            if let Some(dlg) = app.mkdir_dialog.as_mut() {
                dlg.insert(c);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Delete dialog key handling
// ---------------------------------------------------------------------------

fn handle_delete_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.confirm_delete();
        }
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            app.delete_dialog = None;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Shell command dialog key handling
// ---------------------------------------------------------------------------

/// Approximate number of output lines visible in the shell output popup.
const SHELL_VISIBLE_LINES: usize = 20;
/// Lines scrolled per PgUp / PgDn.
const SHELL_PAGE_SIZE: usize = 10;

fn handle_shell_key(app: &mut App, code: KeyCode) {
    let in_output = app
        .shell_dialog
        .as_ref()
        .map(|d| d.output.is_some())
        .unwrap_or(false);

    if in_output {
        let total = app
            .shell_dialog
            .as_ref()
            .and_then(|d| d.output.as_ref())
            .map(|l| l.len())
            .unwrap_or(0);
        match code {
            KeyCode::Esc | KeyCode::Char('q') => { app.shell_dialog = None; }
            KeyCode::Up => {
                if let Some(d) = app.shell_dialog.as_mut() { d.scroll_up(); }
            }
            KeyCode::Down => {
                if let Some(d) = app.shell_dialog.as_mut() {
                    d.scroll_down(total, SHELL_VISIBLE_LINES);
                }
            }
            KeyCode::PageUp => {
                if let Some(d) = app.shell_dialog.as_mut() { d.page_up(SHELL_PAGE_SIZE); }
            }
            KeyCode::PageDown => {
                if let Some(d) = app.shell_dialog.as_mut() {
                    d.page_down(total, SHELL_VISIBLE_LINES, SHELL_PAGE_SIZE);
                }
            }
            _ => {}
        }
    } else {
        match code {
            KeyCode::Esc => { app.shell_dialog = None; }
            KeyCode::Enter => { app.run_shell_command(); }
            KeyCode::Left  => { if let Some(d) = app.shell_dialog.as_mut() { d.move_left(); } }
            KeyCode::Right => { if let Some(d) = app.shell_dialog.as_mut() { d.move_right(); } }
            KeyCode::Home  => { if let Some(d) = app.shell_dialog.as_mut() { d.move_home(); } }
            KeyCode::End   => { if let Some(d) = app.shell_dialog.as_mut() { d.move_end(); } }
            KeyCode::Backspace => {
                if let Some(d) = app.shell_dialog.as_mut() { d.backspace(); }
            }
            KeyCode::Delete => {
                if let Some(d) = app.shell_dialog.as_mut() { d.delete_forward(); }
            }
            KeyCode::Char(c) => {
                if let Some(d) = app.shell_dialog.as_mut() { d.insert(c); }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Field navigation helpers
// ---------------------------------------------------------------------------

/// Total form fields: 0=Name 1=Host 2=Port 3=User 4=Auth 5=KeyPath 6=RemotePath 7=LocalPath
const FORM_FIELDS: usize = 8;

fn next_field(current: usize, auth: &AuthMethod) -> usize {
    let next = (current + 1) % FORM_FIELDS;
    // Skip KeyPath (5) when using Password auth — it is irrelevant.
    if next == 5 && *auth == AuthMethod::Password {
        6
    } else {
        next
    }
}

fn prev_field(current: usize, auth: &AuthMethod) -> usize {
    let prev = if current == 0 { FORM_FIELDS - 1 } else { current - 1 };
    // Skip KeyPath (5) when using Password auth.
    if prev == 5 && *auth == AuthMethod::Password {
        4
    } else {
        prev
    }
}
