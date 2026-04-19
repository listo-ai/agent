//! REST handlers for user and org preferences.
//!
//! Routes (all under `/api/v1`):
//!
//! ```text
//! GET   /me/preferences?org=<id>       resolved view (user ?? org ?? defaults)
//! PATCH /me/preferences?org=<id>       write user layer for that org
//! GET   /orgs/:id/preferences          org layer (admin only)
//! PATCH /orgs/:id/preferences          org layer (admin only)
//! ```
//!
//! `org` query-param defaults to the caller's active tenant when omitted.
//!
//! All timestamps are UTC epoch milliseconds. `PATCH` bodies are sparse —
//! omitted keys leave the stored value unchanged; explicit `null` reverts
//! the field to "inherit from the layer below".

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use data_entities::{OrgPreferences, PreferencesPatch, ResolvedPreferences};
use data_repos::PreferencesService;
use serde::Deserialize;
use spi::{AuthContext, Scope};

use crate::routes::ApiError;
use crate::state::AppState;

// ── Mounting ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/me/preferences",
            get(get_my_preferences).patch(patch_my_preferences),
        )
        .route(
            "/api/v1/orgs/:id/preferences",
            get(get_org_preferences).patch(patch_org_preferences),
        )
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct OrgQuery {
    /// Scope the preferences to this org. Defaults to the caller's active
    /// tenant (taken from the auth context).
    org: Option<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Stable string identifier for the caller — falls back to `"dev-null"` for
/// the dev-null actor so local dev works without a real identity provider.
fn caller_user_id(ctx: &AuthContext) -> String {
    match ctx.actor.node_id() {
        Some(id) => id.to_string(),
        None => "dev-null".to_string(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn require_prefs(state: &AppState) -> Result<&PreferencesService, ApiError> {
    state
        .prefs
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("preferences storage not configured"))
}

// ── GET /api/v1/me/preferences ────────────────────────────────────────────────

async fn get_my_preferences(
    ctx: AuthContext,
    State(s): State<AppState>,
    Query(q): Query<OrgQuery>,
) -> Result<Json<ResolvedPreferences>, ApiError> {
    let svc = require_prefs(&s)?;
    let user_id = caller_user_id(&ctx);
    let org_id = q.org.as_deref().unwrap_or_else(|| ctx.tenant.as_str());
    let resolved = svc
        .resolved(&user_id, org_id)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(resolved))
}

// ── PATCH /api/v1/me/preferences ─────────────────────────────────────────────

async fn patch_my_preferences(
    ctx: AuthContext,
    State(s): State<AppState>,
    Query(q): Query<OrgQuery>,
    Json(patch): Json<PreferencesPatch>,
) -> Result<Json<ResolvedPreferences>, ApiError> {
    let svc = require_prefs(&s)?;
    let user_id = caller_user_id(&ctx);
    let org_id = q.org.unwrap_or_else(|| ctx.tenant.as_str().to_string());
    let resolved = svc
        .patch_user(&user_id, &org_id, patch, now_ms())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(resolved))
}

// ── GET /api/v1/orgs/:id/preferences ─────────────────────────────────────────

async fn get_org_preferences(
    ctx: AuthContext,
    State(s): State<AppState>,
    Path(org_id): Path<String>,
) -> Result<Json<OrgPreferences>, ApiError> {
    ctx.require(Scope::Admin).map_err(ApiError::from_auth)?;
    let svc = require_prefs(&s)?;
    let prefs = svc
        .org_layer(&org_id)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(prefs))
}

// ── PATCH /api/v1/orgs/:id/preferences ───────────────────────────────────────

async fn patch_org_preferences(
    ctx: AuthContext,
    State(s): State<AppState>,
    Path(org_id): Path<String>,
    Json(patch): Json<PreferencesPatch>,
) -> Result<(StatusCode, Json<OrgPreferences>), ApiError> {
    ctx.require(Scope::Admin).map_err(ApiError::from_auth)?;
    let svc = require_prefs(&s)?;
    let updated = svc
        .patch_org(&org_id, patch, now_ms())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((StatusCode::OK, Json(updated)))
}
