//! Block REST surface — reflects [`blocks_host::BlockRegistry`]
//! and serves MF bundles from each block's `ui/` subdir.
//!
//! Per `docs/design/PLUGINS.md` § "Layer B — HTTP surface":
//!
//! ```text
//! GET  /api/v1/blocks                → summary list
//! GET  /api/v1/blocks/:id            → one block
//! POST /api/v1/blocks/:id/enable     → 204
//! POST /api/v1/blocks/:id/disable    → 204
//! POST /api/v1/blocks/reload         → 204  (dev ergonomics)
//! GET  /blocks/:id/*path             → block UI bytes
//! ```

use std::path::{Component, Path as StdPath, PathBuf};

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use blocks_host::{LoadedPluginSummary, BlockId, PluginRuntimeState};

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/blocks", get(list))
        // `reload` + `runtime` must come before the `:id` route;
        // otherwise axum matches them as an :id.
        .route("/api/v1/blocks/reload", post(reload))
        .route("/api/v1/blocks/runtime", get(runtime_all))
        .route("/api/v1/blocks/:id", get(get_one))
        .route("/api/v1/blocks/:id/enable", post(enable))
        .route("/api/v1/blocks/:id/disable", post(disable))
        .route("/api/v1/blocks/:id/runtime", get(runtime_one))
        .route("/blocks/:id/*path", get(serve_ui))
}

async fn list(State(s): State<AppState>) -> Json<Vec<LoadedPluginSummary>> {
    Json(s.blocks.list())
}

fn parse_id(raw: &str) -> Result<BlockId, ApiError> {
    BlockId::parse(raw).map_err(|e| ApiError::bad_request(e.to_string()))
}

async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LoadedPluginSummary>, ApiError> {
    let pid = parse_id(&id)?;
    s.blocks
        .get(&pid)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("no block `{id}`")))
}

async fn enable(State(s): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let pid = parse_id(&id)?;
    // Prefer the host: it flips the registry AND starts the process.
    // Registry-only fallback exists for agents without a writable
    // socket dir (tests, read-only roles).
    if let Some(h) = &s.plugin_host {
        h.enable(&pid)
            .await
            .map_err(|e| ApiError::not_found(e.to_string()))?;
    } else {
        s.blocks
            .set_enabled(&pid, true)
            .map_err(|e| ApiError::not_found(e.to_string()))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn disable(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let pid = parse_id(&id)?;
    if let Some(h) = &s.plugin_host {
        h.disable(&pid)
            .await
            .map_err(|e| ApiError::not_found(e.to_string()))?;
    } else {
        s.blocks
            .set_enabled(&pid, false)
            .map_err(|e| ApiError::not_found(e.to_string()))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Current runtime state (Idle / Starting / Ready / Degraded /
/// Restarting / Failed / Stopped) for one process block.
/// Returns 404 when the block isn't a process block or no host is
/// attached — status only makes sense when there's something to run.
async fn runtime_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PluginRuntimeState>, ApiError> {
    let pid = parse_id(&id)?;
    let host = s
        .plugin_host
        .as_ref()
        .ok_or_else(|| ApiError::not_found("process-block host unavailable"))?;
    host.state(&pid)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("block `{id}` is not running")))
}

/// All process-block runtime states, keyed by id. Empty when there
/// are no process blocks or the host isn't attached.
async fn runtime_all(State(s): State<AppState>) -> Json<Vec<PluginRuntimeEntry>> {
    let Some(host) = &s.plugin_host else {
        return Json(Vec::new());
    };
    let entries = host
        .states()
        .await
        .into_iter()
        .map(|(id, state)| PluginRuntimeEntry { id, state })
        .collect();
    Json(entries)
}

#[derive(serde::Serialize)]
struct PluginRuntimeEntry {
    id: BlockId,
    #[serde(flatten)]
    state: PluginRuntimeState,
}

/// Rescan the blocks directory from disk. Intended as a dev-loop
/// convenience: `make install-block` then `curl -XPOST /reload`
/// without restarting the agent. The graph's block nodes are *not*
/// reconciled here — that's the binary's job on startup; a later
/// stage can fan this out to a reconciliation pass.
async fn reload(State(s): State<AppState>) -> Result<StatusCode, ApiError> {
    s.blocks
        .reload()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Serve a single file from the block's `ui/` subdirectory.
///
/// Intentionally *not* a `ServeDir` — the dir is dynamic per-block
/// and ServeDir doesn't compose cleanly with axum's `State` extraction
/// without pulling the `tower` crate in. Files are small and few
/// (`remoteEntry.js` plus a handful of chunks) so a direct read is
/// fine. Stage 4's Studio host will cache aggressively upstream.
async fn serve_ui(State(s): State<AppState>, Path((id, tail)): Path<(String, String)>) -> Response {
    let pid = match parse_id(&id) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let summary = match s.blocks.get(&pid) {
        Some(sum) => sum,
        None => {
            return ApiError::not_found(format!("no block `{id}`")).into_response();
        }
    };
    if !summary.has_ui {
        return ApiError::not_found(format!("block `{id}` ships no UI bundle")).into_response();
    }
    let Some(root) = plugin_ui_root(&s, &pid) else {
        return ApiError::not_found("blocks dir unavailable").into_response();
    };

    // Validate the tail — reject `..` segments and absolute paths so a
    // client can't escape the block's ui/ directory.
    let rel = StdPath::new(&tail);
    for c in rel.components() {
        match c {
            Component::Normal(_) => {}
            _ => {
                return ApiError::bad_request("invalid path segment").into_response();
            }
        }
    }
    let full = root.join(rel);

    let bytes = match tokio::fs::read(&full).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ApiError::not_found(format!("no asset `{tail}`")).into_response();
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    let mime = mime_for(&full);
    let mut resp = bytes.into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    resp
}

fn plugin_ui_root(s: &AppState, id: &BlockId) -> Option<PathBuf> {
    let dir = s.blocks.blocks_dir()?;
    Some(dir.join(id.as_str()).join("ui"))
}

fn mime_for(p: &StdPath) -> &'static str {
    match p.extension().and_then(|e| e.to_str()) {
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json",
        Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
