//! Error types for domain-backup.

use data_backup::BackupError;

/// Error during snapshot export.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Delegation to data-backup failed.
    #[error("data-backup: {0}")]
    DataBackup(#[from] BackupError),

    /// Generic I/O failure.
    #[error("io: {0}")]
    Io(String),
}

/// Error during snapshot restore / template import.
#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    /// Bundle structure is invalid.
    #[error("invalid bundle: {0}")]
    InvalidBundle(String),

    /// SHA-256 mismatch.
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    /// Device identity doesn't match and `--as-template` wasn't passed.
    #[error("device mismatch: snapshot from {snapshot_device}, target is {target_device}. Pass --as-template to downgrade.")]
    DeviceMismatch {
        snapshot_device: String,
        target_device: String,
    },

    /// Agent version or schema version incompatibility.
    #[error("version incompatible: {0}")]
    VersionIncompatible(String),

    /// Generic I/O failure.
    #[error("io: {0}")]
    Io(String),
}
