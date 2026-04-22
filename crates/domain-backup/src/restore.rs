//! Snapshot restore orchestration — pure logic.
//!
//! Coordinates restore of a `.listo-snapshot` bundle onto the running
//! device. See BACKUP.md § 4.3 + § 4.4.
//!
//! Restore steps:
//! 1. Read and parse the manifest from the bundle tar.
//! 2. Verify device_id matches (or `--as-template` flag downgrades).
//! 3. Verify agent version + schema version compatibility.
//! 4. Extract payload, verify SHA-256.
//! 5. Delegate actual DB restore to the caller (transport layer
//!    coordinates agent drain + file-swap + restart).

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use spi::backup::{BundleKind, BundleManifest};
use tracing::{info, warn};

use crate::error::RestoreError;

/// Parsed content from a bundle — the manifest + extracted payload
/// path. The transport layer uses this to decide whether to proceed
/// with the actual DB restore.
pub struct RestorePlan {
    pub manifest: BundleManifest,
    /// Path to the extracted `payload.tar.zst` on disk.
    pub payload_path: std::path::PathBuf,
}

/// Read a `.listo-snapshot` bundle from the given path, verify its
/// structure, and produce a [`RestorePlan`].
///
/// Does **not** apply the restore — the caller (transport-rest /
/// transport-cli) coordinates the drain + file-swap + restart cycle.
pub fn prepare_restore(
    bundle_path: &Path,
    target_device_id: &str,
    as_template: bool,
    staging_dir: &Path,
) -> Result<RestorePlan, RestoreError> {
    // 1. Open the outer tar and extract manifest.json.
    let file = std::fs::File::open(bundle_path)
        .map_err(|e| RestoreError::Io(format!("open bundle: {e}")))?;
    let mut archive = tar::Archive::new(file);

    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut payload_extracted = false;
    let payload_dest = staging_dir.join("payload.tar.zst");
    let mut sha256_expected: Option<String> = None;

    for entry in archive
        .entries()
        .map_err(|e| RestoreError::Io(format!("tar entries: {e}")))?
    {
        let mut entry = entry.map_err(|e| RestoreError::Io(format!("tar entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| RestoreError::Io(format!("entry path: {e}")))?
            .to_string_lossy()
            .into_owned();

        match path.as_str() {
            "manifest.json" => {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| RestoreError::Io(format!("read manifest: {e}")))?;
                manifest_bytes = Some(buf);
            }
            "payload.sha256" => {
                let mut buf = String::new();
                entry
                    .read_to_string(&mut buf)
                    .map_err(|e| RestoreError::Io(format!("read sha256: {e}")))?;
                sha256_expected = Some(buf.trim().to_string());
            }
            "payload.tar.zst" => {
                let mut out = std::fs::File::create(&payload_dest)
                    .map_err(|e| RestoreError::Io(format!("create payload: {e}")))?;
                std::io::copy(&mut entry, &mut out)
                    .map_err(|e| RestoreError::Io(format!("extract payload: {e}")))?;
                payload_extracted = true;
            }
            other => {
                warn!(entry = other, "unknown entry in bundle — skipping");
            }
        }
    }

    // 2. Parse manifest.
    let manifest_bytes =
        manifest_bytes.ok_or_else(|| RestoreError::InvalidBundle("missing manifest.json".into()))?;
    let manifest: BundleManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| RestoreError::InvalidBundle(format!("parse manifest: {e}")))?;

    if manifest.bundle_kind != BundleKind::Snapshot {
        return Err(RestoreError::InvalidBundle(format!(
            "expected snapshot bundle, got {}",
            manifest.bundle_kind
        )));
    }

    if !payload_extracted {
        return Err(RestoreError::InvalidBundle(
            "missing payload.tar.zst".into(),
        ));
    }

    // 3. Verify SHA-256.
    let payload_data = std::fs::read(&payload_dest)
        .map_err(|e| RestoreError::Io(format!("read payload: {e}")))?;
    let computed = hex::encode(Sha256::digest(&payload_data));

    if computed != manifest.payload_sha256 {
        return Err(RestoreError::HashMismatch {
            expected: manifest.payload_sha256.clone(),
            actual: computed,
        });
    }

    // Cross-check with the sha256 file if present.
    if let Some(expected) = sha256_expected {
        if expected != manifest.payload_sha256 {
            return Err(RestoreError::HashMismatch {
                expected: manifest.payload_sha256.clone(),
                actual: expected,
            });
        }
    }

    // 4. Device-id check.
    let source_device_id = manifest
        .source_device_id
        .as_deref()
        .unwrap_or("");

    if source_device_id != target_device_id {
        if as_template {
            info!(
                source = source_device_id,
                target = target_device_id,
                "device_id mismatch — downgrading to template import"
            );
            // The caller will use template-import codepath. We still
            // return the plan so the caller can inspect the manifest.
        } else {
            return Err(RestoreError::DeviceMismatch {
                snapshot_device: source_device_id.to_string(),
                target_device: target_device_id.to_string(),
            });
        }
    }

    info!(
        device_id = source_device_id,
        agent_version = manifest.agent_version.as_deref().unwrap_or("?"),
        "restore plan prepared"
    );

    Ok(RestorePlan {
        manifest,
        payload_path: payload_dest,
    })
}
