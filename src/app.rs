use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use thiserror::Error;

use crate::config::profiles::{AuthMethod, ConfigError, Profile, ProfileStore};
use crate::connection::sftp::{
    count_files, download_batch, download_file_to_dir, upload_batch, upload_file_fresh,
    SftpConnection, SftpError,
};
use crate::transfer::queue::{
    ProgressHandle, TransferHandle, TransferProgress, TransferState, UploadProgress, UploadState,
};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),
    #[error("SFTP error: {0}")]
    Sftp(#[from] SftpError),
}

/// Which panel is currently focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Left,
    Right,
}

impl ActivePanel {
    pub fn toggle(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// A single entry in a file panel (local or remote)
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub is_dir: bool,
    /// Unix permission string like "rwxr-xr-x" — only set for remote entries
    pub permissions: Option<String>,
}

/// State of a single file panel
#[derive(Debug)]
pub struct PanelState {
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    /// Indices of entries that have been marked with Space.
    pub marked: HashSet<usize>,
}

impl PanelState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
            selected: 0,
            marked: HashSet::new(),
        }
    }

    /// Toggle the mark on the currently highlighted entry (Space key).
    /// The ".." entry cannot be marked.
    pub fn toggle_mark(&mut self) {
        let entry = match self.entries.get(self.selected) {
            Some(e) if e.name != ".." => e,
            _ => return,
        };
        // Entry is valid — toggle its index in the set
        let _ = entry; // satisfy borrow checker; index is self.selected
        if self.marked.contains(&self.selected) {
            self.marked.remove(&self.selected);
        } else {
            self.marked.insert(self.selected);
        }
    }

    /// Mark all non-".." entries. If all are already marked, unmark all (toggle).
    pub fn mark_all(&mut self) {
        let eligible: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name != "..")
            .map(|(i, _)| i)
            .collect();

        if eligible.iter().all(|i| self.marked.contains(i)) {
            // All marked → clear all
            self.marked.clear();
        } else {
            // Some or none marked → mark all eligible
            for i in eligible {
                self.marked.insert(i);
            }
        }
    }

    /// Clear all marks (called when the directory is reloaded).
    pub fn clear_marks(&mut self) {
        self.marked.clear();
    }

    pub fn load_local(&mut self) -> Result<(), AppError> {
        self.entries.clear();
        self.marked.clear();
        if self.path.parent().is_some() {
            self.entries.push(FileEntry {
                name: "..".to_string(),
                size: None,
                modified: None,
                is_dir: true,
                permissions: None,
            });
        }
        let read_dir = std::fs::read_dir(&self.path)?;
        let mut entries: Vec<FileEntry> = read_dir
            .filter_map(|e| e.ok())
            .map(|e| {
                let meta = e.metadata().ok();
                FileEntry {
                    name: e.file_name().to_string_lossy().to_string(),
                    size: meta.as_ref().filter(|m| m.is_file()).map(|m| m.len()),
                    modified: meta.as_ref().and_then(|m| m.modified().ok()),
                    is_dir: meta.map(|m| m.is_dir()).unwrap_or(false),
                    permissions: None,
                }
            })
            .collect();
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        self.entries.extend(entries);
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        Ok(())
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Used for local panel navigation only.
    pub fn enter_selected(&mut self) -> Result<(), AppError> {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.is_dir {
                let new_path = if entry.name == ".." {
                    self.path.parent().unwrap_or(&self.path).to_path_buf()
                } else {
                    self.path.join(&entry.name)
                };
                self.path = new_path;
                self.selected = 0;
                self.load_local()?;
            }
        }
        Ok(())
    }

    /// Navigate to the parent directory (Backspace key) — local only.
    pub fn go_up(&mut self) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent().map(|p| p.to_path_buf()) {
            self.path = parent;
            self.selected = 0;
            self.load_local()?;
        }
        Ok(())
    }

    /// Load remote entries directly into this panel state.
    pub fn load_remote(&mut self, path: PathBuf, entries: Vec<FileEntry>) {
        self.path = path;
        self.entries = entries;
        self.selected = 0;
        self.marked.clear();
    }
}

// ---------------------------------------------------------------------------
// Profile dialog state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileDialogMode {
    List,
    New { field: usize },
    /// Editing an existing profile: `index` is its position in the store.
    Edit { field: usize, index: usize },
    ConfirmDelete { index: usize },
}

#[derive(Debug, Clone)]
pub struct NewProfileForm {
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub auth: AuthMethod,
    pub key_path: String,
    /// Optional remote start directory entered by the user (may be empty).
    pub remote_path: String,
    /// Optional local start directory entered by the user (may be empty).
    pub local_start_path: String,
}

impl NewProfileForm {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            user: String::new(),
            auth: AuthMethod::Key,
            key_path: "~/.ssh/id_rsa".to_string(),
            remote_path: String::new(),
            local_start_path: String::new(),
        }
    }

    /// Return a mutable reference to the string field at `field` index.
    /// Field 4 (Auth toggle) has no string backing — returns None.
    pub fn active_field_mut(&mut self, field: usize) -> Option<&mut String> {
        match field {
            0 => Some(&mut self.name),
            1 => Some(&mut self.host),
            2 => Some(&mut self.port),
            3 => Some(&mut self.user),
            5 => Some(&mut self.key_path),
            6 => Some(&mut self.remote_path),
            7 => Some(&mut self.local_start_path),
            _ => None,
        }
    }

    pub fn to_profile(&self) -> Option<Profile> {
        let port = self.port.parse::<u16>().ok()?;
        if self.name.is_empty() || self.host.is_empty() || self.user.is_empty() {
            return None;
        }
        Some(Profile {
            name: self.name.clone(),
            host: self.host.clone(),
            port,
            user: self.user.clone(),
            auth: self.auth.clone(),
            key_path: if self.key_path.is_empty() {
                None
            } else {
                Some(self.key_path.clone())
            },
            remote_path: if self.remote_path.trim().is_empty() {
                None
            } else {
                Some(self.remote_path.trim().to_string())
            },
            local_start_path: if self.local_start_path.trim().is_empty() {
                None
            } else {
                Some(self.local_start_path.trim().to_string())
            },
        })
    }
}

pub struct ProfileDialog {
    pub mode: ProfileDialogMode,
    pub store: ProfileStore,
    pub list_selected: usize,
    pub form: NewProfileForm,
    pub active_profile: Option<usize>,
}

impl ProfileDialog {
    pub fn new(store: ProfileStore) -> Self {
        Self {
            mode: ProfileDialogMode::List,
            store,
            list_selected: 0,
            form: NewProfileForm::new(),
            active_profile: None,
        }
    }

    pub fn list_move_up(&mut self) {
        if self.list_selected > 0 {
            self.list_selected -= 1;
        }
    }

    pub fn list_move_down(&mut self) {
        let max = self.store.profiles.len().saturating_sub(1);
        if self.list_selected < max {
            self.list_selected += 1;
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        self.store.save()
    }
}

// ---------------------------------------------------------------------------
// Password dialog state
// ---------------------------------------------------------------------------

pub struct PasswordDialog {
    /// The profile we're connecting with
    pub profile: Profile,
    /// Current password input (masked in UI)
    pub input: String,
    pub error: Option<String>,
}

impl PasswordDialog {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            input: String::new(),
            error: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Rename dialog state
// ---------------------------------------------------------------------------

/// Which panel the rename/mkdir/delete applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

pub struct RenameDialog {
    pub side: PanelSide,
    /// Original name of the entry being renamed.
    pub original: String,
    /// Current text in the input field.
    pub input: String,
    /// Byte offset of the cursor inside `input` (always on a char boundary).
    pub cursor_pos: usize,
}

impl RenameDialog {
    pub fn new(side: PanelSide, original: String) -> Self {
        let cursor_pos = original.len(); // start at end
        Self { side, original: original.clone(), input: original, cursor_pos }
    }

    /// Insert a character at the cursor position and advance the cursor.
    pub fn insert(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    /// Delete the character to the left of the cursor (Backspace).
    pub fn backspace(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        // Step back one char boundary
        let mut pos = self.cursor_pos;
        loop {
            pos -= 1;
            if self.input.is_char_boundary(pos) {
                break;
            }
        }
        self.input.remove(pos);
        self.cursor_pos = pos;
    }

    /// Delete the character to the right of the cursor (Delete key).
    pub fn delete_forward(&mut self) {
        if self.cursor_pos >= self.input.len() {
            return;
        }
        self.input.remove(self.cursor_pos);
    }

    /// Move cursor one character to the left.
    pub fn move_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let mut pos = self.cursor_pos;
        loop {
            pos -= 1;
            if self.input.is_char_boundary(pos) {
                break;
            }
        }
        self.cursor_pos = pos;
    }

    /// Move cursor one character to the right.
    pub fn move_right(&mut self) {
        if self.cursor_pos >= self.input.len() {
            return;
        }
        let mut pos = self.cursor_pos + 1;
        while pos <= self.input.len() && !self.input.is_char_boundary(pos) {
            pos += 1;
        }
        self.cursor_pos = pos;
    }

    /// Jump to start of input.
    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Jump to end of input.
    pub fn move_end(&mut self) {
        self.cursor_pos = self.input.len();
    }
}

// ---------------------------------------------------------------------------
// Mkdir dialog state
// ---------------------------------------------------------------------------

pub struct MkdirDialog {
    pub side: PanelSide,
    pub input: String,
    /// Byte offset of the cursor inside `input` (always on a char boundary).
    pub cursor_pos: usize,
}

impl MkdirDialog {
    pub fn new(side: PanelSide) -> Self {
        Self { side, input: String::new(), cursor_pos: 0 }
    }

    /// Insert a character at the cursor position and advance the cursor.
    pub fn insert(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    /// Delete the character to the left of the cursor (Backspace).
    pub fn backspace(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let mut pos = self.cursor_pos;
        loop {
            pos -= 1;
            if self.input.is_char_boundary(pos) {
                break;
            }
        }
        self.input.remove(pos);
        self.cursor_pos = pos;
    }

    /// Delete the character to the right of the cursor (Delete key).
    pub fn delete_forward(&mut self) {
        if self.cursor_pos >= self.input.len() {
            return;
        }
        self.input.remove(self.cursor_pos);
    }

    /// Move cursor one character to the left.
    pub fn move_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let mut pos = self.cursor_pos;
        loop {
            pos -= 1;
            if self.input.is_char_boundary(pos) {
                break;
            }
        }
        self.cursor_pos = pos;
    }

    /// Move cursor one character to the right.
    pub fn move_right(&mut self) {
        if self.cursor_pos >= self.input.len() {
            return;
        }
        let mut pos = self.cursor_pos + 1;
        while pos <= self.input.len() && !self.input.is_char_boundary(pos) {
            pos += 1;
        }
        self.cursor_pos = pos;
    }

    /// Jump to start of input.
    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Jump to end of input.
    pub fn move_end(&mut self) {
        self.cursor_pos = self.input.len();
    }
}

// ---------------------------------------------------------------------------
// Delete dialog state
// ---------------------------------------------------------------------------

pub struct DeleteDialog {
    pub side: PanelSide,
    /// All entries to delete: (name, is_dir).
    /// When a single entry is targeted this Vec has exactly one element.
    pub entries: Vec<(String, bool)>,
}

impl DeleteDialog {
    /// Create a dialog for one or more entries.
    pub fn new_multi(side: PanelSide, entries: Vec<(String, bool)>) -> Self {
        Self { side, entries }
    }
}

// ---------------------------------------------------------------------------
// Edit request (F4)
// ---------------------------------------------------------------------------

/// Describes a pending editor launch produced by `App::prepare_edit`.
/// The main loop consumes this to suspend the terminal, launch the editor,
/// then call `App::finish_edit` on return.
pub enum EditRequest {
    /// A local file — just open in editor, refresh listing after.
    Local {
        path: std::path::PathBuf,
    },
    /// A remote file — temp copy already downloaded; upload back if mtime changed.
    Remote {
        /// Temporary local copy.
        temp_path: std::path::PathBuf,
        /// Original remote path (for upload-back).
        remote_path: std::path::PathBuf,
        /// mtime of temp file before the editor was launched.
        mtime_before: SystemTime,
    },
}

// ---------------------------------------------------------------------------
// Shell command dialog ('!')
// ---------------------------------------------------------------------------

pub struct ShellDialog {
    pub input: String,
    pub cursor_pos: usize,
    /// None = input phase; Some(lines) = output/result phase.
    pub output: Option<Vec<String>>,
    pub scroll: usize,
    pub exit_code: Option<i32>,
}

impl ShellDialog {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            output: None,
            scroll: 0,
            exit_code: None,
        }
    }

    pub fn insert(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos == 0 { return; }
        let mut pos = self.cursor_pos;
        loop { pos -= 1; if self.input.is_char_boundary(pos) { break; } }
        self.input.remove(pos);
        self.cursor_pos = pos;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor_pos < self.input.len() { self.input.remove(self.cursor_pos); }
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos == 0 { return; }
        let mut pos = self.cursor_pos;
        loop { pos -= 1; if self.input.is_char_boundary(pos) { break; } }
        self.cursor_pos = pos;
    }

    pub fn move_right(&mut self) {
        if self.cursor_pos >= self.input.len() { return; }
        let mut pos = self.cursor_pos + 1;
        while pos <= self.input.len() && !self.input.is_char_boundary(pos) { pos += 1; }
        self.cursor_pos = pos;
    }

    pub fn move_home(&mut self) { self.cursor_pos = 0; }
    pub fn move_end(&mut self)  { self.cursor_pos = self.input.len(); }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, total_lines: usize, visible: usize) {
        let max = total_lines.saturating_sub(visible);
        if self.scroll < max { self.scroll += 1; }
    }

    pub fn page_up(&mut self, page: usize) {
        self.scroll = self.scroll.saturating_sub(page);
    }

    pub fn page_down(&mut self, total_lines: usize, visible: usize, page: usize) {
        let max = total_lines.saturating_sub(visible);
        self.scroll = (self.scroll + page).min(max);
    }
}

// ---------------------------------------------------------------------------
// Overall application state
// ---------------------------------------------------------------------------

pub struct App {
    pub left: PanelState,
    pub right: PanelState,
    pub active: ActivePanel,
    pub running: bool,
    pub status_message: Option<String>,
    /// Live SFTP connection (if connected)
    pub sftp: Option<SftpConnection>,
    /// Profile manager dialog
    pub profile_dialog: Option<ProfileDialog>,
    /// Password prompt (shown before connecting with password auth)
    pub password_dialog: Option<PasswordDialog>,
    /// Active upload progress handle (None when idle)
    pub upload_progress: Option<ProgressHandle>,
    /// Active download progress handle (None when idle)
    pub download_progress: Option<TransferHandle>,
    /// Rename dialog (F2)
    pub rename_dialog: Option<RenameDialog>,
    /// Mkdir dialog (F7)
    pub mkdir_dialog: Option<MkdirDialog>,
    /// Delete confirmation dialog (F8)
    pub delete_dialog: Option<DeleteDialog>,
    /// Keyboard shortcut help overlay (F1)
    pub help_visible: bool,
    /// Pending editor launch from F4 — consumed by the main loop.
    pub pending_edit: Option<EditRequest>,
    /// Shell command dialog ('!')
    pub shell_dialog: Option<ShellDialog>,
    /// When true the panels are rendered swapped: remote on the left, local on the right.
    pub panels_swapped: bool,
}

impl App {
    pub fn new() -> Result<Self, AppError> {
        let home = dirs_or_cwd();
        let mut left = PanelState::new(home.clone());
        left.load_local()?;
        let right = PanelState::new(home);
        Ok(Self {
            left,
            right,
            active: ActivePanel::Left,
            running: true,
            status_message: None,
            sftp: None,
            profile_dialog: None,
            password_dialog: None,
            upload_progress: None,
            download_progress: None,
            rename_dialog: None,
            mkdir_dialog: None,
            delete_dialog: None,
            help_visible: false,
            pending_edit: None,
            shell_dialog: None,
            panels_swapped: false,
        })
    }

    pub fn active_panel_mut(&mut self) -> &mut PanelState {
        match self.active {
            ActivePanel::Left => &mut self.left,
            ActivePanel::Right => &mut self.right,
        }
    }

    pub fn toggle_panel(&mut self) {
        self.active = self.active.toggle();
    }

    pub fn quit(&mut self) {
        // Explicitly drop the SFTP connection before exiting so the SSH
        // session is cleanly closed (ssh2 sends a disconnect packet on drop).
        self.sftp = None;
        self.running = false;
    }

    pub fn open_profile_dialog(&mut self) {
        let store = ProfileStore::load().unwrap_or_default();
        self.profile_dialog = Some(ProfileDialog::new(store));
    }

    pub fn close_profile_dialog(&mut self) {
        self.profile_dialog = None;
    }

    /// Initiate connection with a profile. If auth=password, opens the password
    /// dialog first. If auth=key, connects immediately.
    pub fn begin_connect(&mut self, profile: Profile) {
        match profile.auth {
            AuthMethod::Password => {
                self.password_dialog = Some(PasswordDialog::new(profile));
            }
            AuthMethod::Key => {
                self.do_connect(profile, None);
            }
        }
    }

    /// Perform the actual SFTP connect (called after password is entered or for key auth).
    pub fn do_connect(&mut self, profile: Profile, password: Option<&str>) {
        match SftpConnection::connect(&profile, password) {
            Ok(mut conn) => {
                // If the profile specifies a start directory, navigate there first.
                // change_to_absolute returns the new listing directly — use it to
                // avoid a second round-trip and correctly set the panel path.
                let (list_result, connected_msg) =
                    if let Some(ref start_path) = profile.remote_path {
                        let trimmed = start_path.trim();
                        if !trimmed.is_empty() {
                            match conn.change_to_absolute(trimmed) {
                                Ok(entries) => {
                                    let msg = format!(
                                        "Verbunden: {}@{} → {}",
                                        conn.user,
                                        conn.host,
                                        conn.remote_path.display()
                                    );
                                    (Ok(entries), msg)
                                }
                                Err(e) => {
                                    // Fall back to home dir listing
                                    let msg = format!(
                                        "Start-Verzeichnis '{}' nicht erreichbar: {}",
                                        trimmed, e
                                    );
                                    (conn.list_dir(), msg)
                                }
                            }
                        } else {
                            let msg = format!("Verbunden: {}@{}", conn.user, conn.host);
                            (conn.list_dir(), msg)
                        }
                    } else {
                        let msg = format!("Verbunden: {}@{}", conn.user, conn.host);
                        (conn.list_dir(), msg)
                    };

                match list_result {
                    Ok(entries) => {
                        let path = conn.remote_path.clone();
                        self.right.load_remote(path, entries);
                        self.status_message = Some(connected_msg);
                        self.sftp = Some(conn);
                        self.password_dialog = None;
                    }
                    Err(e) => {
                        self.status_message =
                            Some(format!("Verbindung ok, Listing fehlgeschlagen: {}", e));
                        self.sftp = Some(conn);
                        self.password_dialog = None;
                    }
                }

                // If the profile specifies a local start directory, navigate
                // the left panel there (only if the path exists).
                if let Some(ref local_path) = profile.local_start_path {
                    let trimmed = local_path.trim();
                    if !trimmed.is_empty() {
                        let expanded = if trimmed == "~" || trimmed.starts_with("~/") {
                            let home = dirs_or_cwd();
                            if trimmed == "~" {
                                home
                            } else {
                                home.join(&trimmed[2..])
                            }
                        } else {
                            PathBuf::from(trimmed)
                        };
                        if expanded.is_dir() {
                            self.left.path = expanded;
                            self.left.selected = 0;
                            if let Err(e) = self.left.load_local() {
                                if let Some(ref mut msg) = self.status_message {
                                    msg.push_str(&format!(" | Lok. Startpfad fehlgeschlagen: {}", e));
                                }
                            }
                        }
                        // Path doesn't exist → silently keep the current local directory.
                    }
                }
            }
            Err(e) => {
                if let Some(ref mut dlg) = self.password_dialog {
                    dlg.error = Some(e.to_string());
                } else {
                    self.status_message = Some(format!("Verbindung fehlgeschlagen: {}", e));
                }
            }
        }
    }

    /// Disconnect the active SFTP session and clear the right panel.
    pub fn disconnect(&mut self) {
        self.sftp = None;
        let home = dirs_or_cwd();
        self.right = PanelState::new(home);
        self.status_message = Some("Verbindung getrennt".to_string());
    }

    pub fn is_connected(&self) -> bool {
        self.sftp.is_some()
    }

    /// Returns true if an upload is currently running.
    pub fn is_uploading(&self) -> bool {
        self.upload_progress.is_some()
    }

    /// Returns true if a download is currently running.
    pub fn is_downloading(&self) -> bool {
        self.download_progress.is_some()
    }

    /// Returns true if any transfer (upload or download) is running.
    pub fn is_transferring(&self) -> bool {
        self.is_uploading() || self.is_downloading()
    }

    /// Start uploading the marked left-panel entries (or the highlighted entry
    /// when nothing is marked) to the current remote directory.
    /// Does nothing when not connected or an upload is already running.
    pub fn start_upload(&mut self) {
        if !self.is_connected() || self.is_uploading() {
            return;
        }

        // Build the list of entries to upload.
        let entries: Vec<FileEntry> = if self.left.marked.is_empty() {
            // No marks → upload the single highlighted entry.
            match self.left.entries.get(self.left.selected) {
                Some(e) if e.name != ".." => vec![e.clone()],
                _ => return,
            }
        } else {
            // Upload all marked entries (sorted by index for consistency).
            let mut indices: Vec<usize> = self.left.marked.iter().cloned().collect();
            indices.sort_unstable();
            indices
                .iter()
                .filter_map(|&i| self.left.entries.get(i))
                .filter(|e| e.name != "..")
                .cloned()
                .collect()
        };

        if entries.is_empty() {
            return;
        }

        let remote_dir = self.right.path.clone();
        let base_path = self.left.path.clone();

        let (profile, saved_pw) = match &self.sftp {
            Some(conn) => (conn.profile.clone(), conn.saved_password.clone()),
            None => return,
        };

        // Count total files across all entries for the progress bar.
        let total_files: usize = entries
            .iter()
            .map(|e| count_files(&base_path.join(&e.name)))
            .sum::<usize>()
            .max(1);

        let handle: ProgressHandle =
            Arc::new(Mutex::new(UploadProgress::new(total_files)));
        let handle_clone = Arc::clone(&handle);

        let label = if entries.len() == 1 {
            format!("'{}'", entries[0].name)
        } else {
            format!("{} Dateien", entries.len())
        };

        std::thread::spawn(move || {
            upload_batch(
                profile,
                saved_pw,
                entries,
                base_path,
                remote_dir,
                handle_clone,
            );
        });

        self.upload_progress = Some(handle);
        self.status_message = Some(format!("Uploading {}…", label));
        // Clear marks after starting the upload.
        self.left.clear_marks();
    }

    /// Poll the upload handle; refresh remote listing on completion.
    /// Should be called once per render frame.
    pub fn poll_upload(&mut self) {
        let state = match &self.upload_progress {
            Some(h) => h.lock().unwrap().state.clone(),
            None => return,
        };
        match state {
            UploadState::Running => {}
            UploadState::Done => {
                self.upload_progress = None;
                self.status_message = Some("Upload abgeschlossen".to_string());
                // Refresh the remote listing
                if let Some(conn) = self.sftp.as_mut() {
                    match conn.list_dir() {
                        Ok(entries) => {
                            let path = conn.remote_path.clone();
                            self.right.load_remote(path, entries);
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Remote-Aktualisierung fehlgeschlagen: {}", e));
                        }
                    }
                }
            }
            UploadState::Failed(msg) => {
                self.upload_progress = None;
                self.status_message = Some(format!("Upload fehlgeschlagen: {}", msg));
            }
        }
    }

    /// Start downloading the marked right-panel entries (or the highlighted entry
    /// when nothing is marked) to the local directory.
    /// Does nothing when not connected or a transfer is already running.
    pub fn start_download(&mut self) {
        if !self.is_connected() || self.is_transferring() {
            return;
        }

        // Build the list of entries to download.
        let entries: Vec<FileEntry> = if self.right.marked.is_empty() {
            match self.right.entries.get(self.right.selected) {
                Some(e) if e.name != ".." => vec![e.clone()],
                _ => return,
            }
        } else {
            let mut indices: Vec<usize> = self.right.marked.iter().cloned().collect();
            indices.sort_unstable();
            indices
                .iter()
                .filter_map(|&i| self.right.entries.get(i))
                .filter(|e| e.name != "..")
                .cloned()
                .collect()
        };

        if entries.is_empty() {
            return;
        }

        let local_dir = self.left.path.clone();
        let remote_dir = self.right.path.clone();

        let (profile, saved_pw) = match &self.sftp {
            Some(conn) => (conn.profile.clone(), conn.saved_password.clone()),
            None => return,
        };

        // Start with files_total = 1 so the bar shows activity immediately.
        // download_batch will update files_total once it has counted via the
        // same session (no extra connection needed).
        let handle: TransferHandle =
            Arc::new(Mutex::new(TransferProgress::new(1)));
        let handle_clone = Arc::clone(&handle);

        let label = if entries.len() == 1 {
            format!("'{}'", entries[0].name)
        } else {
            format!("{} Dateien", entries.len())
        };

        std::thread::spawn(move || {
            download_batch(
                profile,
                saved_pw,
                entries,
                remote_dir,
                local_dir,
                handle_clone,
            );
        });

        self.download_progress = Some(handle);
        self.status_message = Some(format!("Downloading {}…", label));
        // Clear marks after starting the download.
        self.right.clear_marks();
    }

    /// Poll the download handle; refresh local listing on completion.
    /// Should be called once per render frame.
    pub fn poll_download(&mut self) {
        let state = match &self.download_progress {
            Some(h) => h.lock().unwrap().state.clone(),
            None => return,
        };
        match state {
            TransferState::Running => {}
            TransferState::Done => {
                self.download_progress = None;
                self.status_message = Some("Download abgeschlossen".to_string());
                // Refresh local listing so the new file appears immediately
                if let Err(e) = self.left.load_local() {
                    self.status_message =
                        Some(format!("Lokale Aktualisierung fehlgeschlagen: {}", e));
                }
            }
            TransferState::Failed(msg) => {
                self.download_progress = None;
                self.status_message = Some(format!("Download fehlgeschlagen: {}", msg));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Rename (F2)
    // -----------------------------------------------------------------------

    /// Open the rename dialog for the currently selected entry.
    pub fn open_rename_dialog(&mut self) {
        let side = self.active;
        let panel_side = match side {
            ActivePanel::Left => PanelSide::Left,
            ActivePanel::Right => {
                if !self.is_connected() {
                    return;
                }
                PanelSide::Right
            }
        };
        let panel = match side {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        };
        let entry = match panel.entries.get(panel.selected) {
            Some(e) if e.name != ".." => e.clone(),
            _ => return,
        };
        self.rename_dialog = Some(RenameDialog::new(panel_side, entry.name));
    }

    /// Confirm the rename and apply it.
    pub fn confirm_rename(&mut self) {
        let dlg = match self.rename_dialog.take() {
            Some(d) => d,
            None => return,
        };
        let new_name = dlg.input.trim().to_string();
        if new_name.is_empty() || new_name == dlg.original {
            return;
        }
        match dlg.side {
            PanelSide::Left => {
                let old = self.left.path.join(&dlg.original);
                let new = self.left.path.join(&new_name);
                match std::fs::rename(&old, &new) {
                    Ok(()) => {
                        self.status_message =
                            Some(format!("Umbenannt: {} → {}", dlg.original, new_name));
                        let _ = self.left.load_local();
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Umbenennen fehlgeschlagen: {}", e));
                    }
                }
            }
            PanelSide::Right => {
                if let Some(conn) = self.sftp.as_ref() {
                    match conn.rename(&dlg.original, &new_name) {
                        Ok(()) => {
                            self.status_message =
                                Some(format!("Umbenannt: {} → {}", dlg.original, new_name));
                            if let Some(conn) = self.sftp.as_mut() {
                                match conn.list_dir() {
                                    Ok(entries) => {
                                        let path = conn.remote_path.clone();
                                        self.right.load_remote(path, entries);
                                    }
                                    Err(e) => {
                                        self.status_message =
                                            Some(format!("Listing fehlgeschlagen: {}", e));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Umbenennen fehlgeschlagen: {}", e));
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Mkdir (F7)
    // -----------------------------------------------------------------------

    /// Open the mkdir dialog for the active panel.
    pub fn open_mkdir_dialog(&mut self) {
        let panel_side = match self.active {
            ActivePanel::Left => PanelSide::Left,
            ActivePanel::Right => {
                if !self.is_connected() {
                    return;
                }
                PanelSide::Right
            }
        };
        self.mkdir_dialog = Some(MkdirDialog::new(panel_side));
    }

    /// Confirm directory creation.
    pub fn confirm_mkdir(&mut self) {
        let dlg = match self.mkdir_dialog.take() {
            Some(d) => d,
            None => return,
        };
        let name = dlg.input.trim().to_string();
        if name.is_empty() {
            return;
        }
        match dlg.side {
            PanelSide::Left => {
                let path = self.left.path.join(&name);
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        self.status_message = Some(format!("Verzeichnis '{}' erstellt", name));
                        let _ = self.left.load_local();
                    }
                    Err(e) => {
                        self.status_message =
                            Some(format!("Verzeichnis erstellen fehlgeschlagen: {}", e));
                    }
                }
            }
            PanelSide::Right => {
                if let Some(conn) = self.sftp.as_ref() {
                    match conn.mkdir(&name) {
                        Ok(()) => {
                            self.status_message =
                                Some(format!("Verzeichnis '{}' erstellt", name));
                            if let Some(conn) = self.sftp.as_mut() {
                                match conn.list_dir() {
                                    Ok(entries) => {
                                        let path = conn.remote_path.clone();
                                        self.right.load_remote(path, entries);
                                    }
                                    Err(e) => {
                                        self.status_message =
                                            Some(format!("Listing fehlgeschlagen: {}", e));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Verzeichnis erstellen fehlgeschlagen: {}", e));
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Delete (F8)
    // -----------------------------------------------------------------------

    /// Open the delete confirmation dialog.
    /// If entries are marked, all marked entries are queued for deletion.
    /// Otherwise the single highlighted entry is used.
    pub fn open_delete_dialog(&mut self) {
        let panel_side = match self.active {
            ActivePanel::Left => PanelSide::Left,
            ActivePanel::Right => {
                if !self.is_connected() {
                    return;
                }
                PanelSide::Right
            }
        };
        let panel = match self.active {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        };

        let to_delete: Vec<(String, bool)> = if panel.marked.is_empty() {
            // Single entry — the currently highlighted one
            match panel.entries.get(panel.selected) {
                Some(e) if e.name != ".." => vec![(e.name.clone(), e.is_dir)],
                _ => return,
            }
        } else {
            // All marked entries, sorted by index
            let mut indices: Vec<usize> = panel.marked.iter().cloned().collect();
            indices.sort_unstable();
            indices
                .iter()
                .filter_map(|&i| panel.entries.get(i))
                .filter(|e| e.name != "..")
                .map(|e| (e.name.clone(), e.is_dir))
                .collect()
        };

        if to_delete.is_empty() {
            return;
        }

        self.delete_dialog = Some(DeleteDialog::new_multi(panel_side, to_delete));
    }

    /// Confirm and execute the delete for all entries in the dialog.
    pub fn confirm_delete(&mut self) {
        let dlg = match self.delete_dialog.take() {
            Some(d) => d,
            None => return,
        };

        let total = dlg.entries.len();
        let mut deleted = 0usize;
        let mut last_error: Option<String> = None;

        match dlg.side {
            PanelSide::Left => {
                for (name, is_dir) in &dlg.entries {
                    let path = self.left.path.join(name);
                    let result = if *is_dir {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                    match result {
                        Ok(()) => deleted += 1,
                        Err(e) => last_error = Some(format!("'{}': {}", name, e)),
                    }
                }
                let _ = self.left.load_local();
            }
            PanelSide::Right => {
                if self.sftp.is_none() {
                    return;
                }
                // Delete each entry individually, collecting errors.
                for (name, is_dir) in &dlg.entries {
                    let result = if *is_dir {
                        self.sftp.as_ref().unwrap().delete_dir(name)
                    } else {
                        self.sftp.as_ref().unwrap().delete_file(name)
                    };
                    match result {
                        Ok(()) => deleted += 1,
                        Err(e) => {
                            last_error = Some(format!("'{}': {}", name, e));
                        }
                    }
                }
                // Refresh remote listing after all deletions.
                match self.sftp.as_mut().unwrap().list_dir() {
                    Ok(entries) => {
                        let path = self.sftp.as_ref().unwrap().remote_path.clone();
                        self.right.load_remote(path, entries);
                    }
                    Err(e) => {
                        self.status_message =
                            Some(format!("Listing fehlgeschlagen: {}", e));
                        return;
                    }
                }
            }
        }

        // Status message: show how many were deleted, and the last error if any
        self.status_message = Some(if let Some(err) = last_error {
            format!("{}/{} gelöscht — Fehler: {}", deleted, total, err)
        } else if total == 1 {
            format!("'{}' gelöscht", dlg.entries[0].0)
        } else {
            format!("{} Einträge gelöscht", deleted)
        });

        // Clear marks on the relevant panel
        match dlg.side {
            PanelSide::Left => self.left.clear_marks(),
            PanelSide::Right => self.right.clear_marks(),
        }
    }

    /// Navigate into the selected remote entry (right panel, connected).
    pub fn remote_enter_selected(&mut self) {
        let selected = self.right.selected;
        let entry = match self.right.entries.get(selected) {
            Some(e) => e.clone(),
            None => return,
        };
        if !entry.is_dir {
            return;
        }
        let conn = match self.sftp.as_mut() {
            Some(c) => c,
            None => return,
        };
        match conn.enter_dir(&entry.name) {
            Ok(entries) => {
                let path = conn.remote_path.clone();
                self.right.load_remote(path, entries);
            }
            Err(e) => {
                self.status_message = Some(format!("Verzeichnis öffnen fehlgeschlagen: {}", e));
            }
        }
    }

    /// Navigate to parent on the remote side.
    pub fn remote_go_up(&mut self) {
        let conn = match self.sftp.as_mut() {
            Some(c) => c,
            None => return,
        };
        match conn.go_up() {
            Ok(entries) => {
                let path = conn.remote_path.clone();
                self.right.load_remote(path, entries);
            }
            Err(e) => {
                self.status_message = Some(format!("Verzeichnis wechseln fehlgeschlagen: {}", e));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Edit (F4)
    // -----------------------------------------------------------------------

    /// Prepare an editor launch for the selected file.
    /// For local files the path is returned directly.
    /// For remote files the file is downloaded synchronously to a temp dir.
    /// The result is stored in `self.pending_edit`; the main loop performs the
    /// actual terminal suspend and process spawn.
    pub fn prepare_edit(&mut self) {
        let (panel_side, entry) = match self.active {
            ActivePanel::Left => {
                let e = match self.left.entries.get(self.left.selected) {
                    Some(e) if !e.is_dir && e.name != ".." => e.clone(),
                    _ => {
                        self.status_message = Some("Kein bearbeitbarer Eintrag ausgewählt".into());
                        return;
                    }
                };
                (ActivePanel::Left, e)
            }
            ActivePanel::Right => {
                if !self.is_connected() { return; }
                let e = match self.right.entries.get(self.right.selected) {
                    Some(e) if !e.is_dir && e.name != ".." => e.clone(),
                    _ => {
                        self.status_message = Some("Kein bearbeitbarer Eintrag ausgewählt".into());
                        return;
                    }
                };
                (ActivePanel::Right, e)
            }
        };

        match panel_side {
            ActivePanel::Left => {
                let path = self.left.path.join(&entry.name);
                self.pending_edit = Some(EditRequest::Local { path });
            }
            ActivePanel::Right => {
                let conn = match self.sftp.as_ref() {
                    Some(c) => c,
                    None => return,
                };
                let remote_path = conn.remote_path.join(&entry.name);
                let temp_dir = std::env::temp_dir().join("vela_edit");
                if let Err(e) = std::fs::create_dir_all(&temp_dir) {
                    self.status_message = Some(format!("Temp-Verzeichnis: {}", e));
                    return;
                }
                match download_file_to_dir(conn.sftp(), &remote_path, &temp_dir) {
                    Ok(temp_path) => {
                        let mtime_before = std::fs::metadata(&temp_path)
                            .and_then(|m| m.modified())
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        self.pending_edit = Some(EditRequest::Remote {
                            temp_path,
                            remote_path,
                            mtime_before,
                        });
                    }
                    Err(e) => {
                        self.status_message =
                            Some(format!("Download für Bearbeitung fehlgeschlagen: {}", e));
                    }
                }
            }
        }
    }

    /// Called by the main loop after the editor process has exited.
    /// Checks for changes (remote case), uploads if needed, refreshes listings.
    pub fn finish_edit(&mut self, req: EditRequest) -> Result<(), AppError> {
        match req {
            EditRequest::Local { .. } => {
                self.left.load_local()?;
                self.status_message = Some("Editor geschlossen".to_string());
            }
            EditRequest::Remote { temp_path, remote_path, mtime_before } => {
                let changed = std::fs::metadata(&temp_path)
                    .and_then(|m| m.modified())
                    .map(|t| t > mtime_before)
                    .unwrap_or(false);

                if changed {
                    let (profile, saved_pw) = match self.sftp.as_ref() {
                        Some(c) => (c.profile.clone(), c.saved_password.clone()),
                        None => {
                            let _ = std::fs::remove_file(&temp_path);
                            return Ok(());
                        }
                    };
                    // Use a fresh session: the existing one may have timed out
                    // while the editor was open (SSH2 error -13).
                    match upload_file_fresh(&profile, saved_pw.as_deref(), &temp_path, &remote_path) {
                        Ok(()) => {
                            let name = remote_path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            self.status_message =
                                Some(format!("'{}' hochgeladen", name));
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Upload fehlgeschlagen: {}", e));
                        }
                    }
                    if let Some(conn) = self.sftp.as_mut() {
                        if let Ok(entries) = conn.list_dir() {
                            let path = conn.remote_path.clone();
                            self.right.load_remote(path, entries);
                        }
                    }
                } else {
                    self.status_message = Some("Keine Änderungen, kein Upload".to_string());
                }
                let _ = std::fs::remove_file(&temp_path);
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Shell command ('!')
    // -----------------------------------------------------------------------

    /// Toggle the visual panel swap (Ctrl+U / Ctrl+S).
    pub fn swap_panels(&mut self) {
        self.panels_swapped = !self.panels_swapped;
    }

    pub fn open_shell_dialog(&mut self) {
        self.shell_dialog = Some(ShellDialog::new());
    }

    /// Execute the command currently typed in the shell dialog.
    /// Captures stdout+stderr and switches the dialog to output phase.
    pub fn run_shell_command(&mut self) {
        let cmd = match self.shell_dialog.as_ref() {
            Some(d) if d.output.is_none() => d.input.trim().to_string(),
            _ => return,
        };
        if cmd.is_empty() {
            self.shell_dialog = None;
            return;
        }
        let cwd = self.left.path.clone();
        let result = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .current_dir(&cwd)
            .output();

        let (lines, exit_code) = match result {
            Ok(out) => {
                let mut bytes = out.stdout;
                bytes.extend_from_slice(&out.stderr);
                let text = String::from_utf8_lossy(&bytes).to_string();
                let lines: Vec<String> = if text.is_empty() {
                    vec!["(keine Ausgabe)".to_string()]
                } else {
                    text.lines().map(|l| l.to_string()).collect()
                };
                (lines, out.status.code())
            }
            Err(e) => (vec![format!("Fehler: {}", e)], None),
        };

        if let Some(dlg) = self.shell_dialog.as_mut() {
            dlg.output = Some(lines);
            dlg.scroll = 0;
            dlg.exit_code = exit_code;
        }
        let _ = self.left.load_local();
        let code_str = exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".into());
        self.status_message = Some(format!("! {} — Exit {}", cmd, code_str));
    }
}

fn dirs_or_cwd() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/"))
        })
}
