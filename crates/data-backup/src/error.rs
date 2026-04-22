//! Error types for the data-backup crate.

/// Structured error for backup operations.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// SQLite dump/verify failure.
    #[error("sqlite dump: {0}")]
    SqliteDump(String),

    /// Integrity check failure — the database file is corrupt.
    #[error("integrity check failed on {path}: {detail}")]
    IntegrityCheck { path: String, detail: String },

    /// Generic I/O failure during archive operations.
    #[error("io: {0}")]
    Io(String),

    /// Bundle envelope is structurally invalid (missing manifest,
    /// wrong entry order, etc.).
    #[error("invalid bundle: {0}")]
    InvalidBundle(String),

    /// SHA-256 mismatch between manifest and computed payload hash.
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
}
