//! First-boot setup + edge enrollment endpoints.
//!
//! Per `HOW-TO-ADD-CODE.md` Rule I, this module contains only
//! transport concerns:
//!
//!   1. Extract input (request body, query, headers).
//!   2. Call a domain function — [`domain_auth::SetupService`].
//!   3. Map the outcome to a DTO.
//!   4. Return.
//!
//! All state-machine logic, single-flight serialisation, token
//! generation, graph writes, config writeback, and provider
//! hot-swapping live in `domain-auth`. A future gRPC, CLI, or
//! fleet-transport surface can adopt setup by calling
//! `SetupService::complete_local` — zero logic needs to move.
//!
//! See `docs/design/SYSTEM-BOOTSTRAP.md`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use domain_auth::{setup_node_path, OrgInfo, SetupError, SetupMode, SetupOutcome};
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/setup", post(setup))
        .route("/api/v1/auth/enroll", post(enroll))
}

// ── DTOs ──────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/auth/setup`.
///
/// `admin_password` is accepted in Phase A for forward-compat but
/// never consumed — there is no login-by-password path until Phase B
/// (Zitadel). Storing an unverified secret would be a liability.
#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SetupRequest {
    Cloud {
        org_name: String,
        admin_email: String,
        #[serde(default)]
        admin_password: Option<String>,
    },
    Edge {},
    Standalone {},
}

#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub status: &'static str,
    pub token: String,
    pub advice: &'static str,
    /// Populated only when the operator launched with `--config
    /// <path>` — the agent refuses to rewrite a hand-maintained config
    /// file and returns the YAML fragment for the operator to paste.
    /// Absent when the agent wrote `agent.yaml` itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_snippet: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollRequest {
    pub cloud_url: String,
    pub enrollment_token: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn setup(
    State(s): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<Json<SetupResponse>, ApiError> {
    let svc = s
        .setup
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "setup is not enabled on this role"))?;
    let (mode, org) = split_request(req);
    let outcome = svc
        .complete_local(mode, org)
        .await
        .map_err(setup_error_to_api)?;
    Ok(Json(response_from_outcome(outcome)))
}

async fn enroll(
    State(_s): State<AppState>,
    Json(_req): Json<EnrollRequest>,
) -> Result<Response, ApiError> {
    // Phase A scope gap: the cloud-side `POST /api/v1/agents/enroll`
    // endpoint and the `ZitadelProvider` both land in Phase B. Route
    // is wired now so client + CLI surfaces stay stable across the
    // rollout.
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "edge enrollment requires the Zitadel provider — Phase B. See \
         docs/design/SYSTEM-BOOTSTRAP.md § Phases.",
    ))
}

// ── Mapping ───────────────────────────────────────────────────────────────────

fn split_request(req: SetupRequest) -> (SetupMode, Option<OrgInfo>) {
    match req {
        SetupRequest::Cloud {
            org_name,
            admin_email,
            ..
        } => (
            SetupMode::Cloud,
            Some(OrgInfo {
                org_name,
                admin_email,
            }),
        ),
        SetupRequest::Edge {} => (SetupMode::Edge, None),
        SetupRequest::Standalone {} => (SetupMode::Standalone, None),
    }
}

fn response_from_outcome(o: SetupOutcome) -> SetupResponse {
    SetupResponse {
        status: "ok",
        token: o.token,
        advice: "Store this token — it will not be shown again.",
        config_snippet: o.config_snippet,
    }
}

fn setup_error_to_api(e: SetupError) -> ApiError {
    match e {
        SetupError::AlreadyConfigured { .. } => ApiError::new(StatusCode::CONFLICT, e.to_string()),
        SetupError::ModeMismatch { .. } => ApiError::bad_request(e.to_string()),
        SetupError::SetupNodeMissing(_) => {
            ApiError::new(StatusCode::SERVICE_UNAVAILABLE, e.to_string())
        }
        SetupError::SlotShape(_)
        | SetupError::Graph(_)
        | SetupError::WriteBack(_)
        | SetupError::Serialize(_) => {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    }
}

// ── 503 gate ──────────────────────────────────────────────────────────────────

/// Paths that remain reachable while `/agent/setup.status ==
/// "unconfigured"`. Explicit allow-list — a blocklist would miss new
/// routes by default.
const SETUP_MODE_ALLOWLIST: &[&str] = &[
    "/api/v1/auth/setup",
    // Orchestrator liveness probes must not return 503 during setup
    // or the agent will be restart-looped.
    "/healthz",
    // Capability self-report is static and non-sensitive.
    "/api/v1/capabilities",
];

/// Axum middleware that returns `503 not_configured` for every
/// non-allowlisted path while setup is pending. No-op when
/// [`AppState::setup`] is `None` (non-setup roles) or when the
/// service reports `is_configured() == true`.
///
/// The gate consults [`domain_auth::SetupService::status`] per
/// request — cheap (one slot lookup). Avoids caching because a setup
/// completion must be visible to every subsequent request with no
/// cache-invalidation window.
pub async fn gate_setup_mode(
    State(s): State<AppState>,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let Some(svc) = s.setup.as_ref() else {
        return next.run(request).await;
    };
    if svc.is_configured() {
        return next.run(request).await;
    }
    let path = request.uri().path();
    if SETUP_MODE_ALLOWLIST.iter().any(|p| *p == path) {
        return next.run(request).await;
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "not_configured",
            "message": "Run POST /api/v1/auth/setup first.",
            "setup_path": setup_node_path().to_string(),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_error_mapping_covers_every_variant() {
        // Ensures no variant is dropped into the catch-all 500 by
        // accident if SetupError grows — compiler enforces exhaustive
        // match inside `setup_error_to_api`.
        let a = setup_error_to_api(SetupError::AlreadyConfigured {
            actual: "local".into(),
        });
        assert_eq!(a.status, StatusCode::CONFLICT);

        let b = setup_error_to_api(SetupError::ModeMismatch {
            expected: "edge".into(),
            requested: "cloud".into(),
        });
        assert_eq!(b.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn response_preserves_token_and_snippet() {
        let r = response_from_outcome(SetupOutcome {
            token: "abc".to_string(),
            config_snippet: Some("auth:\n  provider: static_token\n".to_string()),
        });
        assert_eq!(r.token, "abc");
        assert!(r.config_snippet.is_some());
    }
}
