// STAGE-1 complete — data-backup: SQLite dump, bundle writing, verification

#![forbid(unsafe_code)]
//! `data-backup` — database dump helpers and bundle I/O.
//!
//! Data-layer crate: SQLite dump via `VACUUM INTO` + integrity check,
//! tar + zstd bundle writing, SHA-256 hashing. Writes to any
//! `io::Write`. **No dependency on `ArtifactStore`.** See BACKUP.md
//! § 6.1.
//!
//! ## Modules
//!
//! * [`sqlite`] — SQLite dump + verify utilities.
//! * [`bundle`] — tar/zstd envelope writer (shared by snapshot and
//!   template).
//! * [`error`] — error types.

pub mod bundle;
pub mod error;
pub mod sqlite;

pub use bundle::{compress_payload, write_bundle};
pub use error::BackupError;
pub use sqlite::{dump_sqlite, verify_sqlite};
