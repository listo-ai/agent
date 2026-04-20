//! Plugin REST surface — reflects [`extensions_host::PluginRegistry`]
//! and serves MF bundles from each plugin's `ui/` subdir.
//!
//! Per `docs/design/PLUGINS.md` § "Layer B — HTTP surface":
//!
//! ```text
//! GET  /api/v1/plugins                → summary list
//! GET  /api/v1/plugins/:id            → one plugin
//! POST /api/v1/plugins/:id/enable     → 204
//! POST /api/v1/plugins/:id/disable    → 204
//! POST /api/v1/plugins/reload         → 204  (dev ergonomics)
//! GET  /plugins/:id/*path             → plugin UI bytes
//! ```

use std::path::{Component, Path as StdPath, PathBuf};

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use extensions_host::{LoadedPluginSummary, PluginId, PluginRuntimeState};

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/plugins", get(list))
        // `reload` + `runtime` must come before the `:id` route;
        // otherwise axum matches them as an :id.
        .route("/api/v1/plugins/reload", post(reload))
        .route("/api/v1/plugins/runtime", get(runtime_all))
        .route("/api/v1/plugins/:id", get(get_one))
        .route("/api/v1/plugins/:id/enable", post(enable))
        .route("/api/v1/plugins/:id/disable", post(disable))
        .route("/api/v1/plugins/:id/runtime", get(runtime_one))
        .route("/plugins/:id/*path", get(serve_ui))
}

async fn list(State(s): State<AppState>) -> Json<Vec<LoadedPluginSummary>> {
    Json(s.plugins.list())
}

fn parse_id(raw: &str) -> Result<PluginId, ApiError> {
    PluginId::parse(raw).map_err(|e| ApiError::bad_request(e.to_string()))
}

async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LoadedPluginSummary>, ApiError> {
    let pid = parse_id(&id)?;
    s.plugins
        .get(&pid)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("no plugin `{id}`")))
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
        s.plugins
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
        s.plugins
            .set_enabled(&pid, false)
            .map_err(|e| ApiError::not_found(e.to_string()))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Current runtime state (Idle / Starting / Ready / Degraded /
/// Restarting / Failed / Stopped) for one process plugin.
/// Returns 404 when the plugin isn't a process plugin or no host is
/// attached — status only makes sense when there's something to run.
async fn runtime_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PluginRuntimeState>, ApiError> {
    let pid = parse_id(&id)?;
    let host = s
        .plugin_host
        .as_ref()
        .ok_or_else(|| ApiError::not_found("process-plugin host unavailable"))?;
    host.state(&pid)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("plugin `{id}` is not running")))
}

/// All process-plugin runtime states, keyed by id. Empty when there
/// are no process plugins or the host isn't attached.
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
    id: PluginId,
    #[serde(flatten)]
    state: PluginRuntimeState,
}

/// Rescan the plugins directory from disk. Intended as a dev-loop
/// convenience: `make install-plugin` then `curl -XPOST /reload`
/// without restarting the agent. The graph's plugin nodes are *not*
/// reconciled here — that's the binary's job on startup; a later
/// stage can fan this out to a reconciliation pass.
async fn reload(State(s): State<AppState>) -> Result<StatusCode, ApiError> {
    s.plugins
        .reload()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Serve a single file from the plugin's `ui/` subdirectory.
///
/// Intentionally *not* a `ServeDir` — the dir is dynamic per-plugin
/// and ServeDir doesn't compose cleanly with axum's `State` extraction
/// without pulling the `tower` crate in. Files are small and few
/// (`remoteEntry.js` plus a handful of chunks) so a direct read is
/// fine. Stage 4's Studio host will cache aggressively upstream.
async fn serve_ui(State(s): State<AppState>, Path((id, tail)): Path<(String, String)>) -> Response {
    let pid = match parse_id(&id) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let summary = match s.plugins.get(&pid) {
        Some(sum) => sum,
        None => {
            return ApiError::not_found(format!("no plugin `{id}`")).into_response();
        }
    };
    if !summary.has_ui {
        return ApiError::not_found(format!("plugin `{id}` ships no UI bundle")).into_response();
    }
    let Some(root) = plugin_ui_root(&s, &pid) else {
        return ApiError::not_found("plugins dir unavailable").into_response();
    };

    // Validate the tail — reject `..` segments and absolute paths so a
    // client can't escape the plugin's ui/ directory.
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

fn plugin_ui_root(s: &AppState, id: &PluginId) -> Option<PathBuf> {
    let dir = s.plugins.plugins_dir()?;
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
