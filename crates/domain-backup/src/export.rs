//! Snapshot export orchestration — pure logic.
//!
//! Coordinates the steps to produce a `.listo-snapshot` bundle:
//! 1. Dump SQLite via `data-backup`.
//! 2. Stage files into a temp directory.
//! 3. Compress payload (tar + zstd).
//! 4. Build manifest.
//! 5. Write outer bundle envelope.
//!
//! This module has **no HTTP, no SQL** — it delegates I/O to
//! `data-backup` and writes to a caller-supplied path.
//! See BACKUP.md § 6 and CODE-LAYOUT.md.

use std::path::{Path, PathBuf};

use spi::backup::{BundleManifest, SchemaVersions};
use tracing::info;

use crate::error::ExportError;

/// Inputs for a snapshot export. The transport layer fills these from
/// CLI flags or HTTP request bodies — domain-backup neither opens
/// databases nor constructs paths from config.
pub struct SnapshotExportInput {
    /// Path to the live SQLite database file.
    pub sqlite_path: PathBuf,
    /// The `device_id` of this agent (from listod's claim).
    pub device_id: String,
    /// Agent binary version (e.g. `"0.42.1"`).
    pub agent_version: String,
    /// Current hostname (advisory — not trusted for identity).
    pub hostname: String,
    /// Scratch directory for staging the dump + payload.
    /// Caller creates and cleans up.
    pub staging_dir: PathBuf,
}

/// Result of a successful snapshot export.
pub struct SnapshotExportResult {
    pub bundle_path: PathBuf,
    pub manifest: BundleManifest,
}

/// Run the full snapshot export pipeline, writing the bundle to
/// `dest_path`.
///
/// Steps follow BACKUP.md § 4.4 (export direction):
/// 1. Dump SQLite via `VACUUM INTO` into `staging_dir`.
/// 2. Compress staging into `payload.tar.zst`.
/// 3. Build the manifest with SHA-256 + schema version.
/// 4. Write the outer bundle envelope to `dest_path`.
pub fn export_snapshot(
    input: &SnapshotExportInput,
    dest_path: &Path,
) -> Result<SnapshotExportResult, ExportError> {
    // 1. Dump SQLite.
    let sqlite_dump_path = input.staging_dir.join("state.sqlite");
    let sqlite_version = data_backup::dump_sqlite(&input.sqlite_path, &sqlite_dump_path)
        .map_err(ExportError::DataBackup)?;

    info!(sqlite_version, "SQLite dumped");

    // 2. Compress payload.
    let payload_path = input.staging_dir.join("payload.tar.zst");
    let payload_sha256 = data_backup::compress_payload(&input.staging_dir, &payload_path)
        .map_err(ExportError::DataBackup)?;

    // 3. Build manifest.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let tool = format!("agent@{}", input.agent_version);
    let manifest = BundleManifest::new_snapshot(
        input.device_id.clone(),
        input.agent_version.clone(),
        payload_sha256,
        now_ms,
        tool,
    )
    .with_hostname(&input.hostname)
    .with_schema(SchemaVersions {
        sqlite: sqlite_version,
        postgres: 0, // Edge tier — no Postgres.
    });

    // 4. Write outer bundle.
    let file = std::fs::File::create(dest_path).map_err(|e| {
        ExportError::Io(format!("create {}: {e}", dest_path.display()))
    })?;
    data_backup::write_bundle(file, &manifest, &payload_path)
        .map_err(ExportError::DataBackup)?;

    info!(
        path = %dest_path.display(),
        device_id = %input.device_id,
        "snapshot exported"
    );

    Ok(SnapshotExportResult {
        bundle_path: dest_path.to_path_buf(),
        manifest,
    })
}
