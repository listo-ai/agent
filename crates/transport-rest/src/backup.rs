//! REST handlers for snapshot backup and restore.
//!
//! Routes (all under `/api/v1`):
//!
//! ```text
//! POST  /backup/snapshot/export   export a full snapshot to a local path
//! POST  /backup/snapshot/import   validate a snapshot bundle
//! POST  /backup/template/export   export a portability-filtered template  (Phase 2)
//! POST  /backup/template/import   plan a template import (Phase 2)
//! ```
//!
//! When the `artifacts-local` Cargo feature is active, snapshot export also
//! accepts `local://<directory>` destination URLs and pipes the bundle into a
//! `LocalArtifactStore` via `domain_artifacts::upload_stream`.

use std::collections::{HashMap, HashSet};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use domain_backup::{ExportError, RestoreError, SnapshotExportInput, TemplateExportInput, TemplateNode};
use serde::{Deserialize, Serialize};
use spi::backup::Portability;

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/backup/snapshot/export", post(snapshot_export))
        .route("/api/v1/backup/snapshot/import", post(snapshot_import))
        .route("/api/v1/backup/template/export", post(template_export))
        .route("/api/v1/backup/template/import", post(template_import))
}

// ── Export ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ExportReq {
    destination: String,
}

#[derive(Serialize)]
struct ExportResp {
    path: String,
    size_bytes: u64,
    sha256: String,
}

async fn snapshot_export(
    State(s): State<AppState>,
    Json(req): Json<ExportReq>,
) -> Result<Json<ExportResp>, ApiError> {
    let db_path = s.db_path.clone().ok_or_else(|| {
        ApiError::bad_request("agent is running in memory-only mode — backup not available")
    })?;

    // ── destination routing ────────────────────────────────────────────────

    // When the `artifacts-local` feature is active, `local://<dir>` ships the
    // bundle directly into a LocalArtifactStore.  All other URL schemes are
    // rejected; plain paths continue to work unconditionally.
    #[cfg(feature = "artifacts-local")]
    if let Some(store_root) = req.destination.strip_prefix("local://") {
        return snapshot_export_local_artifact(s, db_path, store_root.to_string()).await;
    }

    if req.destination.contains("://") {
        return Err(ApiError::bad_request(
            "URL schemes are not supported — use a local filesystem path or local://<dir>",
        ));
    }

    let dest = std::path::PathBuf::from(&req.destination);
    let device_id = s.device_id.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<_, ExportError> {
        let staging_dir = tempfile::tempdir()
            .map_err(|e| ExportError::Io(format!("staging dir: {e}")))?;

        let hostname = std::fs::read_to_string("/etc/hostname")
            .map(|h| h.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let input = SnapshotExportInput {
            sqlite_path: db_path,
            device_id,
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            hostname,
            staging_dir: staging_dir.path().to_path_buf(),
        };

        domain_backup::export_snapshot(&input, &dest)
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("task: {e}")))?
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let size_bytes = std::fs::metadata(&result.bundle_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(Json(ExportResp {
        path: result.bundle_path.to_string_lossy().into_owned(),
        size_bytes,
        sha256: result.manifest.payload_sha256.clone(),
    }))
}

// ── Import ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ImportReq {
    bundle_path: String,
    #[serde(default)]
    as_template: bool,
}

#[derive(Serialize)]
struct ImportResp {
    status: String,
    agent_version: Option<String>,
    source_device_id: Option<String>,
    as_template: bool,
}

async fn snapshot_import(
    State(s): State<AppState>,
    Json(req): Json<ImportReq>,
) -> Result<Response, ApiError> {
    let bundle_path = std::path::PathBuf::from(&req.bundle_path);
    let device_id = s.device_id.clone();
    let as_template = req.as_template;

    let plan = tokio::task::spawn_blocking(move || -> Result<_, RestoreError> {
        let staging_dir = tempfile::tempdir()
            .map_err(|e| RestoreError::Io(format!("staging dir: {e}")))?;

        domain_backup::prepare_restore(
            &bundle_path,
            &device_id,
            as_template,
            staging_dir.path(),
        )
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("task: {e}")))?
    .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let as_template_result = as_template
        || plan
            .manifest
            .source_device_id
            .as_deref()
            .map(|id| id != s.device_id.as_str())
            .unwrap_or(false);

    // Phase 1: validation only — actual DB-swap + restart arrives in Phase 2.
    // Return 202 Accepted so clients know things validated but nothing was applied yet.
    Ok((
        StatusCode::ACCEPTED,
        Json(ImportResp {
            status: "validated".into(),
            agent_version: plan.manifest.agent_version.clone(),
            source_device_id: plan.manifest.source_device_id.clone(),
            as_template: as_template_result,
        }),
    )
        .into_response())
}

// ── Template export ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TemplateExportReq {
    destination: String,
}

#[derive(Serialize)]
struct TemplateExportResp {
    path: String,
    size_bytes: u64,
    node_count: u32,
    sha256: String,
}

async fn template_export(
    State(s): State<AppState>,
    Json(req): Json<TemplateExportReq>,
) -> Result<Json<TemplateExportResp>, ApiError> {
    if req.destination.contains("://") {
        return Err(ApiError::bad_request(
            "URL schemes are not supported — use a local filesystem path",
        ));
    }

    // Walk the graph and build portability-filtered TemplateNode list.
    let nodes = build_template_nodes(&s)?;
    let node_count = nodes.len() as u32;
    let dest = std::path::PathBuf::from(&req.destination);
    let agent_version = env!("CARGO_PKG_VERSION").to_string();

    let result = tokio::task::spawn_blocking(move || -> Result<_, ExportError> {
        let staging_dir = tempfile::tempdir()
            .map_err(|e| ExportError::Io(format!("staging dir: {e}")))?;

        let input = TemplateExportInput {
            nodes,
            agent_version,
            staging_dir: staging_dir.path().to_path_buf(),
        };

        domain_backup::export_template(&input, &dest)
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("task: {e}")))?
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let size_bytes = std::fs::metadata(&result.bundle_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(Json(TemplateExportResp {
        path: result.bundle_path.to_string_lossy().into_owned(),
        size_bytes,
        node_count,
        sha256: result.manifest.payload_sha256.clone(),
    }))
}

/// Walk all graph nodes and return only Portable slot values.
fn build_template_nodes(s: &AppState) -> Result<Vec<TemplateNode>, ApiError> {
    let snapshots = s.graph.snapshots();
    let mut out = Vec::with_capacity(snapshots.len());

    for snap in snapshots {
        let manifest = s.graph.kinds().get(&snap.kind);

        // Build portability map: slot_name -> Portability
        let portability_map: HashMap<String, Portability> = manifest
            .as_ref()
            .map(|m| {
                m.slots
                    .iter()
                    .map(|slot| (slot.name.clone(), slot.portability))
                    .collect()
            })
            .unwrap_or_default();

        // Include only Portable slot values.
        let slots: HashMap<String, serde_json::Value> = snap
            .slot_values
            .iter()
            .filter(|(name, _)| {
                portability_map
                    .get(name.as_str())
                    .copied()
                    .unwrap_or(Portability::Portable) // default: portable
                    == Portability::Portable
            })
            .map(|(name, sv)| (name.clone(), sv.value.clone()))
            .collect();

        out.push(TemplateNode {
            path: snap.path.to_string(),
            kind: snap.kind.as_str().to_string(),
            slots,
        });
    }

    Ok(out)
}

// ── Template import ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TemplateImportReq {
    bundle_path: String,
    #[serde(default = "default_strategy")]
    strategy: domain_backup::ConflictStrategy,
    /// Optional namespace prefix for the `namespace` strategy (Phase 2 full wiring).
    #[allow(dead_code)]
    #[serde(default)]
    namespace: Option<String>,
}

fn default_strategy() -> domain_backup::ConflictStrategy {
    domain_backup::ConflictStrategy::Merge
}

async fn template_import(
    State(s): State<AppState>,
    Json(req): Json<TemplateImportReq>,
) -> Result<Json<domain_backup::ImportPlan>, ApiError> {
    let bundle_path = std::path::PathBuf::from(&req.bundle_path);
    let device_id = s.device_id.clone();
    let existing_paths: HashSet<String> = s
        .graph
        .snapshots()
        .into_iter()
        .map(|n| n.path.to_string())
        .collect();
    let strategy = req.strategy;

    let plan = tokio::task::spawn_blocking(move || -> Result<_, RestoreError> {
        let staging_dir = tempfile::tempdir()
            .map_err(|e| RestoreError::Io(format!("staging dir: {e}")))?;

        // Validate the bundle as a template (as_template=true skips device
        // check and reads template.json instead of a SQLite dump).
        domain_backup::prepare_restore(
            &bundle_path,
            &device_id,
            true, // templates have no device_id we need to match
            staging_dir.path(),
        )?;

        // Parse template.json from the extracted payload.
        let doc = domain_backup::read_template_doc(staging_dir.path())?;

        Ok(domain_backup::plan_import(&doc.nodes, &existing_paths, strategy))
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("task: {e}")))?
    .map_err(|e| ApiError::bad_request(e.to_string()))?;

    Ok(Json(plan))
}

// ── Local artifact store integration (feature = "artifacts-local") ────────────

/// Export a snapshot and upload it into a `LocalArtifactStore` rooted at
/// `store_root`.  Destination URL format: `local://<absolute-or-relative-dir>`.
///
/// The artifact key follows `snapshots/<device_id>/<ts_ms>.listo-snapshot`.
#[cfg(feature = "artifacts-local")]
async fn snapshot_export_local_artifact(
    s: AppState,
    db_path: std::path::PathBuf,
    store_root: String,
) -> Result<Json<ExportResp>, ApiError> {
    use data_artifacts_local::{LocalArtifactStore, LocalConfig};
    use domain_artifacts::upload_stream;

    let device_id = s.device_id.clone();

    // 1. Export snapshot to a temp file.
    let (bundle_path, sha256) =
        tokio::task::spawn_blocking(move || -> Result<_, ExportError> {
            let staging_dir = tempfile::tempdir()
                .map_err(|e| ExportError::Io(format!("staging dir: {e}")))?;
            let bundle_dir = tempfile::tempdir()
                .map_err(|e| ExportError::Io(format!("bundle dir: {e}")))?;
            let bundle_dest =
                bundle_dir.path().join(format!("{device_id}.listo-snapshot"));

            let hostname = std::fs::read_to_string("/etc/hostname")
                .map(|h| h.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            let input = SnapshotExportInput {
                sqlite_path: db_path,
                device_id: device_id.clone(),
                agent_version: env!("CARGO_PKG_VERSION").to_string(),
                hostname,
                staging_dir: staging_dir.path().to_path_buf(),
            };

            let result = domain_backup::export_snapshot(&input, &bundle_dest)?;
            let sha256 = result.manifest.payload_sha256.clone();
            // Keep the dirs alive until we return the path.
            std::mem::forget(staging_dir);
            std::mem::forget(bundle_dir);
            Ok((result.bundle_path, sha256))
        })
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("task: {e}")))?
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 2. Open the bundle file and stream it into LocalArtifactStore.
    let store_path = std::path::PathBuf::from(&store_root);
    let store = LocalArtifactStore::new(LocalConfig { root: store_path })
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let device_id = s.device_id.clone();
    let key = format!("snapshots/{device_id}/{ts_ms}.listo-snapshot");

    let file = tokio::fs::File::open(&bundle_path)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let size_bytes = file
        .metadata()
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    upload_stream(&store, &key, file)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Clean up the temp bundle file.
    let _ = tokio::fs::remove_file(&bundle_path).await;

    Ok(Json(ExportResp {
        path: format!("local://{store_root}/{key}"),
        size_bytes,
        sha256,
    }))
}
