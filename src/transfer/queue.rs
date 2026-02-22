use std::sync::{Arc, Mutex};

/// Current state of a running transfer (upload or download).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferState {
    /// Transfer is in progress.
    Running,
    /// Transfer finished successfully.
    Done,
    /// Transfer failed with an error message.
    Failed(String),
}

// Backwards-compat aliases used by the upload code.
pub use TransferState as UploadState;

/// Shared progress state — written by the transfer thread, read by the render loop.
#[derive(Debug)]
pub struct TransferProgress {
    pub state: TransferState,
    /// Name of the file currently being transferred.
    pub current_file: String,
    /// Bytes transferred for the current file.
    pub bytes_done: u64,
    /// Total bytes of the current file (0 if unknown / directory).
    pub bytes_total: u64,
    /// Number of files fully transferred so far.
    pub files_done: usize,
    /// Total number of files to transfer.
    pub files_total: usize,
}

// Backwards-compat alias used by the upload code.
pub use TransferProgress as UploadProgress;

impl TransferProgress {
    pub fn new(files_total: usize) -> Self {
        Self {
            state: TransferState::Running,
            current_file: String::new(),
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total,
        }
    }

    /// 0.0 – 1.0 progress fraction for the current file.
    #[allow(dead_code)]
    pub fn file_fraction(&self) -> f64 {
        if self.bytes_total == 0 {
            0.0
        } else {
            (self.bytes_done as f64 / self.bytes_total as f64).clamp(0.0, 1.0)
        }
    }

    /// 0.0 – 1.0 overall progress fraction (by file count).
    pub fn overall_fraction(&self) -> f64 {
        if self.files_total == 0 {
            1.0
        } else {
            (self.files_done as f64 / self.files_total as f64).clamp(0.0, 1.0)
        }
    }
}

/// A thread-safe handle to transfer progress.
pub type TransferHandle = Arc<Mutex<TransferProgress>>;

// Backwards-compat alias used by the upload code.
pub use TransferHandle as ProgressHandle;
