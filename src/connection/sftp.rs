use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use ssh2::{FileStat, OpenFlags, OpenType, Session, Sftp};
use thiserror::Error;

use crate::app::FileEntry;
use crate::config::profiles::{AuthMethod, Profile};
use crate::transfer::queue::{ProgressHandle, TransferHandle, TransferState, UploadState};

#[derive(Debug, Error)]
pub enum SftpError {
    #[error("TCP connection failed: {0}")]
    Tcp(#[from] std::io::Error),
    #[error("SSH error: {0}")]
    Ssh(#[from] ssh2::Error),
    #[error("Authentication failed")]
    AuthFailed,
    #[error("Key file not found: {0}")]
    KeyNotFound(String),
    #[error("Remote path error: {0}")]
    Path(String),
}

/// An active SFTP session.
pub struct SftpConnection {
    // Session must be kept alive alongside Sftp.
    _session: Session,
    sftp: Sftp,
    pub remote_path: PathBuf,
    /// The login home directory — never changes after connect.
    /// Used by `change_to_absolute` to expand `~`.
    home: PathBuf,
    pub host: String,
    pub user: String,
    /// Stored so the upload thread can open a second session.
    pub profile: Profile,
    /// Stored password (only set for password-auth profiles).
    pub saved_password: Option<String>,
}

impl SftpConnection {
    /// Establish an SFTP connection using a profile.
    /// `password` is only used when `profile.auth == AuthMethod::Password`.
    pub fn connect(profile: &Profile, password: Option<&str>) -> Result<Self, SftpError> {
        let addr = format!("{}:{}", profile.host, profile.port);
        let tcp = TcpStream::connect(&addr)?;
        // 10-second connect + read timeout
        tcp.set_read_timeout(Some(Duration::from_secs(10)))?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        authenticate(&mut session, profile, password)?;

        let sftp = session.sftp()?;

        // Resolve the remote home directory (realpath of ".").
        let home = resolve_home(&sftp)?;

        Ok(Self {
            _session: session,
            sftp,
            remote_path: home.clone(),
            home,
            host: profile.host.clone(),
            user: profile.user.clone(),
            profile: profile.clone(),
            saved_password: password.map(|s| s.to_string()),
        })
    }

    /// List the current remote directory. Returns entries sorted: dirs first, then files.
    pub fn list_dir(&self) -> Result<Vec<FileEntry>, SftpError> {
        let mut entries: Vec<FileEntry> = Vec::new();

        // Always add ".." unless we are at the root "/"
        if self.remote_path != PathBuf::from("/") {
            entries.push(FileEntry {
                name: "..".to_string(),
                size: None,
                modified: None,
                is_dir: true,
                permissions: None,
            });
        }

        let raw = self
            .sftp
            .readdir(&self.remote_path)
            .map_err(|e| SftpError::Path(e.to_string()))?;

        let mut dir_entries: Vec<FileEntry> = raw
            .into_iter()
            .map(|(path, stat)| file_entry_from_stat(path, &stat))
            .collect();

        dir_entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        entries.extend(dir_entries);
        Ok(entries)
    }

    /// Change into a subdirectory and return the new listing.
    pub fn enter_dir(&mut self, name: &str) -> Result<Vec<FileEntry>, SftpError> {
        let new_path = if name == ".." {
            self.remote_path
                .parent()
                .unwrap_or(&self.remote_path)
                .to_path_buf()
        } else {
            self.remote_path.join(name)
        };
        self.remote_path = new_path;
        self.list_dir()
    }

    /// Switch to an absolute remote path and return the new listing.
    /// Expands a leading `~` to the login home directory that was resolved
    /// right after connecting (stored in `self.home`).
    pub fn change_to_absolute(&mut self, raw: &str) -> Result<Vec<FileEntry>, SftpError> {
        let expanded = if raw == "~" || raw.starts_with("~/") {
            let home_str = self.home.to_string_lossy().to_string();
            if raw == "~" {
                home_str
            } else {
                // raw starts with "~/" → replace prefix
                format!("{}{}", home_str, &raw[1..])
            }
        } else {
            raw.to_string()
        };

        // Use realpath to canonicalise the path (resolves symlinks, "..", etc.)
        // and simultaneously verify that it exists on the server.
        let canonical = self
            .sftp
            .realpath(std::path::Path::new(&expanded))
            .map_err(|e| SftpError::Path(format!("Pfad nicht gefunden '{}': {}", expanded, e)))?;

        // Confirm it is a directory.
        let stat = self
            .sftp
            .stat(&canonical)
            .map_err(|e| SftpError::Path(format!("stat fehlgeschlagen: {}", e)))?;

        if !stat.file_type().is_dir() {
            return Err(SftpError::Path(format!(
                "'{}' ist kein Verzeichnis",
                canonical.display()
            )));
        }

        self.remote_path = canonical;
        self.list_dir()
    }

    /// Return a reference to the inner SFTP handle for synchronous operations
    /// (e.g. the F4 edit flow that downloads/uploads without a separate session).
    pub fn sftp(&self) -> &Sftp {
        &self.sftp
    }

    /// Navigate to the parent directory.
    pub fn go_up(&mut self) -> Result<Vec<FileEntry>, SftpError> {
        if let Some(parent) = self.remote_path.parent().map(|p| p.to_path_buf()) {
            self.remote_path = parent;
        }
        self.list_dir()
    }

    /// Rename (or move) an entry in the current remote directory.
    pub fn rename(&self, old_name: &str, new_name: &str) -> Result<(), SftpError> {
        let old = self.remote_path.join(old_name);
        let new = self.remote_path.join(new_name);
        self.sftp
            .rename(&old, &new, None)
            .map_err(|e| SftpError::Path(e.to_string()))
    }

    /// Create a new directory in the current remote directory.
    pub fn mkdir(&self, name: &str) -> Result<(), SftpError> {
        let path = self.remote_path.join(name);
        self.sftp
            .mkdir(&path, 0o755)
            .map_err(|e| SftpError::Path(e.to_string()))
    }

    /// Delete a file in the current remote directory.
    pub fn delete_file(&self, name: &str) -> Result<(), SftpError> {
        let path = self.remote_path.join(name);
        self.sftp
            .unlink(&path)
            .map_err(|e| SftpError::Path(format!("{}: {}", path.display(), e)))
    }

    /// Recursively delete a directory and all its contents.
    pub fn delete_dir(&self, name: &str) -> Result<(), SftpError> {
        let path = self.remote_path.join(name);
        self.rmdir_recursive(&path)
    }

    /// Internal recursive removal: depth-first, files before dirs.
    fn rmdir_recursive(&self, path: &std::path::Path) -> Result<(), SftpError> {
        let entries = self
            .sftp
            .readdir(path)
            .map_err(|e| SftpError::Path(e.to_string()))?;

        for (child, stat) in entries {
            if stat.file_type().is_dir() {
                self.rmdir_recursive(&child)?;
            } else {
                self.sftp
                    .unlink(&child)
                    .map_err(|e| SftpError::Path(e.to_string()))?;
            }
        }
        self.sftp
            .rmdir(path)
            .map_err(|e| SftpError::Path(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Upload — runs inside a dedicated thread with its own SSH session
// ---------------------------------------------------------------------------

/// Open a **single** SSH+SFTP session and upload all `entries` from
/// `local_dir` to `remote_dir`, reporting progress through `handle`.
/// On success the state is set to `Done`; on failure to `Failed`.
pub fn upload_batch(
    profile: Profile,
    password: Option<String>,
    entries: Vec<crate::app::FileEntry>,
    local_dir: PathBuf,
    remote_dir: PathBuf,
    handle: ProgressHandle,
) {
    let result = (|| -> Result<(), SftpError> {
        let addr = format!("{}:{}", profile.host, profile.port);
        let tcp = TcpStream::connect(&addr)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        authenticate(&mut session, &profile, password.as_deref())?;

        let sftp = session.sftp()?;

        for entry in &entries {
            // Abort if a previous entry already failed.
            {
                let h = handle.lock().unwrap();
                if matches!(h.state, UploadState::Failed(_)) {
                    return Ok(());
                }
            }
            let local = local_dir.join(&entry.name);
            if local.is_dir() {
                upload_dir_recursive(&sftp, &local, &remote_dir, &handle)?;
            } else {
                upload_file(&sftp, &local, &remote_dir, &handle)?;
            }
        }
        Ok(())
    })();

    let mut prog = handle.lock().unwrap();
    match result {
        Ok(()) => {
            if matches!(prog.state, UploadState::Running) {
                prog.state = UploadState::Done;
            }
        }
        Err(e) => {
            prog.state = UploadState::Failed(e.to_string());
        }
    }
}

/// Count the total number of regular files under a path (recursive).
pub fn count_files(path: &Path) -> usize {
    if path.is_file() {
        return 1;
    }
    let Ok(rd) = std::fs::read_dir(path) else {
        return 0;
    };
    rd.filter_map(|e| e.ok())
        .map(|e| count_files(&e.path()))
        .sum()
}

/// Upload a single file to `remote_dir/filename`.
fn upload_file(
    sftp: &Sftp,
    local: &Path,
    remote_dir: &Path,
    handle: &ProgressHandle,
) -> Result<(), SftpError> {
    let name = local
        .file_name()
        .ok_or_else(|| SftpError::Path("no filename".into()))?;
    let remote_path = remote_dir.join(name);

    let metadata = std::fs::metadata(local)?;
    let total = metadata.len();

    {
        let mut prog = handle.lock().unwrap();
        prog.current_file = name.to_string_lossy().to_string();
        prog.bytes_done = 0;
        prog.bytes_total = total;
    }

    let mut local_file = std::fs::File::open(local)?;
    let mut remote_file = sftp
        .open_mode(
            &remote_path,
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| SftpError::Path(e.to_string()))?;

    let mut buf = vec![0u8; 64 * 1024]; // 64 KiB chunks
    loop {
        let n = local_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        remote_file
            .write_all(&buf[..n])
            .map_err(|e| SftpError::Path(e.to_string()))?;

        let mut prog = handle.lock().unwrap();
        prog.bytes_done = (prog.bytes_done + n as u64).min(total);
    }

    {
        let mut prog = handle.lock().unwrap();
        prog.files_done += 1;
    }

    Ok(())
}

/// Recursively upload a directory tree.
fn upload_dir_recursive(
    sftp: &Sftp,
    local_dir: &Path,
    remote_parent: &Path,
    handle: &ProgressHandle,
) -> Result<(), SftpError> {
    let dir_name = local_dir
        .file_name()
        .ok_or_else(|| SftpError::Path("no dirname".into()))?;
    let remote_dir = remote_parent.join(dir_name);

    // Create remote directory (ignore "already exists" error)
    match sftp.mkdir(&remote_dir, 0o755) {
        Ok(()) => {}
        Err(e) if e.code() == ssh2::ErrorCode::SFTP(4) => {} // SSH_FX_FAILURE = already exists
        Err(e) => return Err(SftpError::Path(e.to_string())),
    }

    let read_dir = std::fs::read_dir(local_dir)?;
    for entry in read_dir.filter_map(|e| e.ok()) {
        let child = entry.path();
        if child.is_dir() {
            upload_dir_recursive(sftp, &child, &remote_dir, handle)?;
        } else {
            upload_file(sftp, &child, &remote_dir, handle)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Download — runs inside a dedicated thread with its own SSH session
// ---------------------------------------------------------------------------

/// Open a **single** SSH+SFTP session and download all `entries` from
/// `remote_dir` into `local_dir`, reporting progress through `handle`.
/// After counting files the handle's `files_total` is updated so the
/// progress bar shows accurate percentages from the start.
/// On success the state is set to `Done`; on failure to `Failed`.
pub fn download_batch(
    profile: Profile,
    password: Option<String>,
    entries: Vec<crate::app::FileEntry>,
    remote_dir: PathBuf,
    local_dir: PathBuf,
    handle: TransferHandle,
) {
    let result = (|| -> Result<(), SftpError> {
        let addr = format!("{}:{}", profile.host, profile.port);
        let tcp = TcpStream::connect(&addr)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        authenticate(&mut session, &profile, password.as_deref())?;

        let sftp = session.sftp()?;

        // Count total files upfront using the same session (no extra connection).
        let total: usize = entries
            .iter()
            .map(|e| count_sftp_files(&sftp, &remote_dir.join(&e.name)))
            .sum::<usize>()
            .max(1);
        {
            let mut h = handle.lock().unwrap();
            h.files_total = total;
        }

        // Download all entries over the same session.
        for entry in &entries {
            // Abort if a previous entry already failed.
            {
                let h = handle.lock().unwrap();
                if matches!(h.state, TransferState::Failed(_)) {
                    return Ok(());
                }
            }
            let remote = remote_dir.join(&entry.name);
            let stat = sftp
                .stat(&remote)
                .map_err(|e| SftpError::Path(e.to_string()))?;
            if stat.file_type().is_dir() {
                download_dir_recursive(&sftp, &remote, &local_dir, &handle)?;
            } else {
                download_file(&sftp, &remote, &local_dir, &handle)?;
            }
        }
        Ok(())
    })();

    let mut prog = handle.lock().unwrap();
    match result {
        Ok(()) => {
            if matches!(prog.state, TransferState::Running) {
                prog.state = TransferState::Done;
            }
        }
        Err(e) => {
            prog.state = TransferState::Failed(e.to_string());
        }
    }
}


pub(crate) fn count_sftp_files(sftp: &Sftp, remote: &Path) -> usize {
    let stat = match sftp.stat(remote) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if !stat.file_type().is_dir() {
        return 1;
    }
    let entries = match sftp.readdir(remote) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    entries
        .iter()
        .map(|(p, _)| count_sftp_files(sftp, p))
        .sum()
}

/// Download a single remote file into `local_dir/filename`.
fn download_file(
    sftp: &Sftp,
    remote: &Path,
    local_dir: &Path,
    handle: &TransferHandle,
) -> Result<(), SftpError> {
    let name = remote
        .file_name()
        .ok_or_else(|| SftpError::Path("no filename".into()))?;
    let local_path = local_dir.join(name);

    // Get remote file size for progress (best-effort)
    let total = sftp
        .stat(remote)
        .ok()
        .and_then(|s| s.size)
        .unwrap_or(0);

    {
        let mut prog = handle.lock().unwrap();
        prog.current_file = name.to_string_lossy().to_string();
        prog.bytes_done = 0;
        prog.bytes_total = total;
    }

    let mut remote_file = sftp
        .open(remote)
        .map_err(|e| SftpError::Path(e.to_string()))?;

    let mut local_file = std::fs::File::create(&local_path)?;

    let mut buf = vec![0u8; 64 * 1024]; // 64 KiB chunks
    loop {
        let n = remote_file
            .read(&mut buf)
            .map_err(|e| SftpError::Path(e.to_string()))?;
        if n == 0 {
            break;
        }
        local_file.write_all(&buf[..n])?;

        let mut prog = handle.lock().unwrap();
        prog.bytes_done = if total > 0 {
            (prog.bytes_done + n as u64).min(total)
        } else {
            prog.bytes_done + n as u64
        };
    }

    {
        let mut prog = handle.lock().unwrap();
        prog.files_done += 1;
    }

    Ok(())
}

/// Recursively download a remote directory tree into `local_parent`.
fn download_dir_recursive(
    sftp: &Sftp,
    remote_dir: &Path,
    local_parent: &Path,
    handle: &TransferHandle,
) -> Result<(), SftpError> {
    let dir_name = remote_dir
        .file_name()
        .ok_or_else(|| SftpError::Path("no dirname".into()))?;
    let local_dir = local_parent.join(dir_name);

    // Create local directory (ignore "already exists")
    match std::fs::create_dir(&local_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(SftpError::Tcp(e)),
    }

    let entries = sftp
        .readdir(remote_dir)
        .map_err(|e| SftpError::Path(e.to_string()))?;

    for (remote_child, stat) in entries {
        if stat.file_type().is_dir() {
            download_dir_recursive(sftp, &remote_child, &local_dir, handle)?;
        } else {
            download_file(sftp, &remote_child, &local_dir, handle)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Synchronous single-file helpers (used by the F4 edit flow)
// ---------------------------------------------------------------------------

/// Download a single remote file into `local_dir` using an **existing** SFTP
/// handle.  No progress reporting — intended for the synchronous edit flow.
/// Returns the path of the created local file.
pub(crate) fn download_file_to_dir(
    sftp: &Sftp,
    remote: &Path,
    local_dir: &Path,
) -> Result<PathBuf, SftpError> {
    let name = remote
        .file_name()
        .ok_or_else(|| SftpError::Path("no filename".into()))?;
    let local_path = local_dir.join(name);

    let mut remote_file = sftp
        .open(remote)
        .map_err(|e| SftpError::Path(e.to_string()))?;
    let mut local_file = std::fs::File::create(&local_path)?;

    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = remote_file
            .read(&mut buf)
            .map_err(|e| SftpError::Path(e.to_string()))?;
        if n == 0 {
            break;
        }
        local_file.write_all(&buf[..n])?;
    }
    Ok(local_path)
}

/// Upload a single local file to an explicit `remote_path` using an
/// **existing** SFTP handle.  Overwrites the remote file if it exists.
pub(crate) fn upload_file_to_path(
    sftp: &Sftp,
    local: &Path,
    remote: &Path,
) -> Result<(), SftpError> {
    let mut local_file = std::fs::File::open(local)?;
    let mut remote_file = sftp
        .open_mode(
            remote,
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| SftpError::Path(e.to_string()))?;

    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = local_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        remote_file
            .write_all(&buf[..n])
            .map_err(|e| SftpError::Path(e.to_string()))?;
    }
    Ok(())
}

/// Open a **fresh** SSH+SFTP session and upload a single local file to
/// `remote_path`.  Used by the F4 edit flow where the existing session may
/// have timed out while the editor was open.
pub fn upload_file_fresh(
    profile: &Profile,
    password: Option<&str>,
    local: &Path,
    remote: &Path,
) -> Result<(), SftpError> {
    let addr = format!("{}:{}", profile.host, profile.port);
    let tcp = TcpStream::connect(&addr)?;
    tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;
    authenticate(&mut session, profile, password)?;

    let sftp = session.sftp()?;
    upload_file_to_path(&sftp, local, remote)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn authenticate(
    session: &mut Session,
    profile: &Profile,
    password: Option<&str>,
) -> Result<(), SftpError> {
    match &profile.auth {
        AuthMethod::Key => {
            let key_path_raw = profile
                .key_path
                .as_deref()
                .unwrap_or("~/.ssh/id_rsa");
            let key_path = expand_tilde(key_path_raw);
            if !key_path.exists() {
                return Err(SftpError::KeyNotFound(
                    key_path.display().to_string(),
                ));
            }
            session
                .userauth_pubkey_file(&profile.user, None, &key_path, None)
                .map_err(|_| SftpError::AuthFailed)?;
        }
        AuthMethod::Password => {
            let pw = password.unwrap_or("");
            session
                .userauth_password(&profile.user, pw)
                .map_err(|_| SftpError::AuthFailed)?;
        }
    }

    if !session.authenticated() {
        return Err(SftpError::AuthFailed);
    }
    Ok(())
}

fn resolve_home(sftp: &Sftp) -> Result<PathBuf, SftpError> {
    // "." resolves to the user's home on most SSH servers
    let canonical = sftp
        .realpath(std::path::Path::new("."))
        .map_err(|e| SftpError::Path(e.to_string()))?;
    Ok(canonical)
}

fn file_entry_from_stat(path: PathBuf, stat: &FileStat) -> FileEntry {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let is_dir = stat
        .file_type()
        .is_dir();

    let size = if is_dir { None } else { stat.size };

    let modified = stat.mtime.map(|t| {
        UNIX_EPOCH + Duration::from_secs(t)
    });

    let permissions = stat.perm.map(format_permissions);

    FileEntry {
        name,
        size,
        modified,
        is_dir,
        permissions,
    }
}

/// Convert a Unix mode bitmask into a `rwxr-xr-x` style string.
fn format_permissions(mode: u32) -> String {
    let flags = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    let mut s = String::with_capacity(9);
    for (bit, ch) in &flags {
        s.push(if mode & bit != 0 { *ch } else { '-' });
    }
    s
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(rest)
    } else if path == "~" {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
    } else {
        PathBuf::from(path)
    }
}
