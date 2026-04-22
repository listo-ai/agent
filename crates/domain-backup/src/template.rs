//! Template export/import — portability-filtered node graph.
//!
//! A template is a `.listo-template` bundle whose payload is a
//! `template.json` document (not a SQLite dump). Only slots declared
//! with `Portability::Portable` are included; `Device`, `Derived`, and
//! `Secret` slots are stripped before shipping.
//!
//! ## Conflict strategies (BACKUP.md § 3)
//!
//! | Strategy    | Behaviour                                                  |
//! |-------------|-----------------------------------------------------------|
//! | `namespace` | Prefix every incoming path. Never conflicts.              |
//! | `merge`     | Add new paths; for overlapping paths diff + classify.     |
//! | `overwrite` | Delete-and-recreate every node in the incoming set.       |

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::backup::{BundleKind, BundleManifest};
use tracing::info;

use crate::error::ExportError;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single node as it appears in `template.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateNode {
    /// Absolute path, e.g. `/station/boiler-1`.
    pub path: String,
    /// Kind id, e.g. `com.acme.pid-controller`.
    pub kind: String,
    /// Portable slot values keyed by slot name.
    /// Slots absent here are either non-Portable or had null value.
    #[serde(default)]
    pub slots: HashMap<String, JsonValue>,
}

/// The document stored as `template.json` inside the bundle payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateDoc {
    pub schema_version: u32,
    pub node_count: u32,
    pub nodes: Vec<TemplateNode>,
}

/// Transport-supplied inputs for template export.
///
/// The transport layer (handler / CLI) is responsible for reading the
/// graph and filtering slots by portability before constructing this
/// struct — domain-backup has no graph access.
pub struct TemplateExportInput {
    /// Pre-filtered nodes: only Portable slot values included.
    pub nodes: Vec<TemplateNode>,
    /// Agent version string.
    pub agent_version: String,
    /// Scratch directory for staging. Caller creates; domain-backup
    /// may write into it but does not clean up.
    pub staging_dir: std::path::PathBuf,
}

/// Successful result of a template export.
pub struct TemplateExportResult {
    pub bundle_path: std::path::PathBuf,
    pub manifest: BundleManifest,
}

/// Conflict resolution strategy for template import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Prefix every incoming path with a caller-supplied namespace so it
    /// never collides with existing nodes.
    Namespace,
    /// Classify per path: *add* (new), *skip* (identical), *conflict*
    /// (present and different). Caller resolves conflicts.
    Merge,
    /// Delete-and-recreate: existing nodes at matching paths are wiped
    /// before import. Gated by G2 on the real API surface; Phase 2
    /// allows it from trusted CLI/fleet callers.
    Overwrite,
}

/// A conflicting path in a `merge` plan.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub path: String,
    pub existing_kind: String,
    pub incoming_kind: String,
    /// True when the kind differs — operator must resolve manually.
    pub kind_changed: bool,
}

/// The plan produced by [`plan_import`].
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportPlan {
    pub strategy: String,
    /// Paths that do not exist on the target — will be created.
    pub adds: Vec<String>,
    /// Paths whose kind is identical — slots will be merged/overwritten.
    pub updates: Vec<String>,
    /// Paths skipped because the node is byte-identical.
    pub skips: Vec<String>,
    /// Paths with kind changes or other issues requiring manual review.
    pub conflicts: Vec<ConflictEntry>,
    /// Non-fatal warnings (e.g. unknown kinds, stripped secrets).
    pub warnings: Vec<String>,
}

// ── Export ────────────────────────────────────────────────────────────────────

/// Export a template bundle to `dest_path`.
///
/// Steps:
/// 1. Serialise the node list to `template.json` in `staging_dir`.
/// 2. Compress the staging dir into `payload.tar.zst`.
/// 3. Build the manifest.
/// 4. Write the outer envelope to `dest_path`.
pub fn export_template(
    input: &TemplateExportInput,
    dest_path: &Path,
) -> Result<TemplateExportResult, ExportError> {
    let doc = TemplateDoc {
        schema_version: 1,
        node_count: input.nodes.len() as u32,
        nodes: input.nodes.clone(),
    };

    // 1. Write template.json into staging.
    let template_path = input.staging_dir.join("template.json");
    let json = serde_json::to_vec_pretty(&doc)
        .map_err(|e| ExportError::Io(format!("serialise template: {e}")))?;
    std::fs::write(&template_path, &json)
        .map_err(|e| ExportError::Io(format!("write template.json: {e}")))?;

    // 2. Compress payload.
    let payload_path = input.staging_dir.join("payload.tar.zst");
    let payload_sha256 =
        data_backup::compress_payload(&input.staging_dir, &payload_path)
            .map_err(ExportError::DataBackup)?;

    // 3. Build manifest.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let tool = format!("agent@{}", input.agent_version);
    let manifest = BundleManifest::new_template(1, payload_sha256, now_ms, tool)
        .with_node_count(input.nodes.len() as u32);

    // 4. Write outer bundle.
    let file = std::fs::File::create(dest_path)
        .map_err(|e| ExportError::Io(format!("create {}: {e}", dest_path.display())))?;
    data_backup::write_bundle(file, &manifest, &payload_path)
        .map_err(ExportError::DataBackup)?;

    info!(
        path = %dest_path.display(),
        nodes = input.nodes.len(),
        "template exported"
    );

    Ok(TemplateExportResult {
        bundle_path: dest_path.to_path_buf(),
        manifest,
    })
}

// ── Import planning ───────────────────────────────────────────────────────────

/// Produce an [`ImportPlan`] for importing `incoming` nodes onto a target
/// that already has `existing_paths`.
///
/// Does **not** apply anything — the transport layer uses the plan to
/// drive the actual graph mutations and report to the operator.
pub fn plan_import(
    incoming: &[TemplateNode],
    existing_paths: &HashSet<String>,
    strategy: ConflictStrategy,
) -> ImportPlan {
    let mut adds = Vec::new();
    let mut updates = Vec::new();
    let mut skips = Vec::new();
    let mut conflicts = Vec::new();
    let warnings = Vec::new();

    match strategy {
        ConflictStrategy::Namespace => {
            // Every node is treated as an add (no collision check).
            for node in incoming {
                adds.push(node.path.clone());
            }
        }
        ConflictStrategy::Merge => {
            for node in incoming {
                if existing_paths.contains(&node.path) {
                    // In a full implementation we'd diff slot values.
                    // Phase 2: mark as update; detailed diff is Phase 3.
                    updates.push(node.path.clone());
                } else {
                    adds.push(node.path.clone());
                }
            }
        }
        ConflictStrategy::Overwrite => {
            for node in incoming {
                if existing_paths.contains(&node.path) {
                    updates.push(node.path.clone());
                } else {
                    adds.push(node.path.clone());
                }
            }
        }
    }

    ImportPlan {
        strategy: format!("{strategy:?}").to_lowercase(),
        adds,
        updates,
        skips,
        conflicts,
        warnings,
    }
}

/// Read the `template.json` from a staged `.listo-template` bundle.
///
/// Expects `staging_dir` to contain `payload.tar.zst` as placed by
/// [`crate::restore::prepare_restore`]. Decompresses the payload in
/// memory to extract and parse `template.json`.
pub fn read_template_doc(staging_dir: &Path) -> Result<TemplateDoc, crate::error::RestoreError> {
    use std::io::Read;

    let payload_path = staging_dir.join("payload.tar.zst");
    let file = std::fs::File::open(&payload_path)
        .map_err(|e| crate::error::RestoreError::Io(format!("open payload: {e}")))?;

    let decoder = zstd::Decoder::new(file)
        .map_err(|e| crate::error::RestoreError::Io(format!("zstd decoder: {e}")))?;
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .map_err(|e| crate::error::RestoreError::Io(format!("tar entries: {e}")))?
    {
        let mut entry = entry
            .map_err(|e| crate::error::RestoreError::Io(format!("tar entry: {e}")))?;
        let name = entry
            .path()
            .map_err(|e| crate::error::RestoreError::Io(format!("entry path: {e}")))?
            .to_string_lossy()
            .into_owned();

        if name == "template.json" {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)
                .map_err(|e| crate::error::RestoreError::Io(format!("read template.json: {e}")))?;
            return serde_json::from_slice(&buf).map_err(|e| {
                crate::error::RestoreError::InvalidBundle(format!("parse template.json: {e}"))
            });
        }
    }

    Err(crate::error::RestoreError::InvalidBundle(
        "template.json not found in payload".into(),
    ))
}
