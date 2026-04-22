//! First-boot setup + edge enrollment endpoints.
//!
//! `POST /api/v1/auth/setup` — runs exactly once per agent process
//! while `/agent/setup.status == "unconfigured"`. Generates a strong
//! random bearer token, persists it (depending on
//! [`SetupWriteback`]), hot-swaps the live
//! [`spi::AuthProvider`] to a `StaticTokenProvider` populated with the
//! new entry, and flips `/agent/setup.status` to `"local"`. The second
//! concurrent caller observes the transition under the single-flight
//! mutex and gets `409 already_configured` — never a second token.
//!
//! `POST /api/v1/auth/enroll` — edge-only. Phase A wires the route +
//! client surface but returns `501 Not Implemented` pointing at Phase
//! B. The cloud-side enroll endpoint and the `ZitadelProvider` need to
//! exist before enroll can complete end-to-end. See
//! `docs/design/SYSTEM-BOOTSTRAP.md` § Phases.

use std::str::FromStr;
use std::sync::Arc;

use auth::{StaticTokenEntry, StaticTokenProvider};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use config::{AgentConfigOverlay, AuthOverlay, StaticTokenOverlay};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::{Actor, NodeId, NodePath, Scope, TenantId};

use crate::routes::ApiError;
use crate::state::{AppState, SetupWriteback};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/setup", post(setup))
        .route("/api/v1/auth/enroll", post(enroll))
}

/// Paths that remain reachable while `/agent/setup.status ==
/// "unconfigured"`. Everything else returns 503. Kept explicit
/// (allow-list) because the tail of what can safely run during setup
/// is very short — reviewing a blocklist would miss new routes by
/// default.
const SETUP_MODE_ALLOWLIST: &[&str] = &[
    "/api/v1/auth/setup",
    // Operators frequently hit `/healthz` from orchestrators; failing
    // liveness during first-boot would trigger restarts that never
    // resolve. It returns a string literal and touches nothing
    // sensitive.
    "/healthz",
    "/api/v1/capabilities",
];

/// Axum middleware that gates every non-allowlisted path with 503
/// while setup is pending. Checks the `/agent/setup.status` slot per
/// request — cheap (one hashmap lookup on the graph), so avoids the
/// complexity of subscribing to slot change events just to cache the
/// value. Once the setup handler flips status to `"local"`, this
/// middleware becomes a no-op for every subsequent request.
pub async fn gate_setup_mode(
    axum::extract::State(s): axum::extract::State<AppState>,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path();
    if SETUP_MODE_ALLOWLIST.iter().any(|p| *p == path) {
        return next.run(request).await;
    }
    // Read directly — if the node isn't seeded (wrong role, dev
    // fixture, etc.) we treat that as "not in setup mode" and let the
    // request through. The setup route itself seeds the node at boot
    // before the router mounts, so a missing node in production is
    // already an indication that setup isn't in play.
    let setup_path = setup_node_path();
    let status = match s.graph.get(&setup_path) {
        None => return next.run(request).await,
        Some(snap) => snap
            .slot_values
            .iter()
            .find(|(n, _)| n == "status")
            .and_then(|(_, v)| v.value.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string()),
    };
    if status == "unconfigured" {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "not_configured",
                "message": "Run POST /api/v1/auth/setup first.",
            })),
        )
            .into_response();
    }
    next.run(request).await
}

// ---- setup ---------------------------------------------------------------

/// Request body for `POST /api/v1/auth/setup`.
///
/// `admin_password` is accepted for forward-compat but **ignored in
/// Phase A** — no login-by-password path exists until Phase B (Zitadel)
/// lands. Storing an unverified secret would be a liability with no
/// benefit. See `docs/design/SYSTEM-BOOTSTRAP.md` § "Transport security
/// requirement".
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

/// Response from a successful setup call.
#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub status: &'static str,
    pub token: String,
    pub advice: &'static str,
    /// Populated only when the operator launched with `--config <path>`.
    /// The agent refuses to rewrite a hand-maintained config file; the
    /// operator must paste this snippet themselves. `None` when the
    /// agent wrote `agent.yaml` on their behalf.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_snippet: Option<String>,
}

async fn setup(
    State(s): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<Json<SetupResponse>, ApiError> {
    // Single-flight: the second caller waits here, then observes the
    // `status != "unconfigured"` transition under the lock and gets
    // 409. No second token is ever generated.
    let _guard = s.setup_guard.lock().await;

    let setup_path = setup_node_path();
    let status = read_status(&s, &setup_path)?;
    if status != "unconfigured" {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!("already_configured (status=`{status}`)"),
        ));
    }

    let request_mode = match &req {
        SetupRequest::Cloud { .. } => "cloud",
        SetupRequest::Edge {} => "edge",
        SetupRequest::Standalone {} => "standalone",
    };
    let node_mode = read_mode(&s, &setup_path)?;
    if node_mode != request_mode {
        return Err(ApiError::bad_request(format!(
            "mode_mismatch — agent was started in `{node_mode}` mode \
             but setup request claimed `{request_mode}`"
        )));
    }

    // Generate + install in-memory state BEFORE touching disk so a
    // writeback failure doesn't leave a ghost token that authenticates
    // this process only until restart. If the writeback fails we bail
    // before the status flip, and the operator gets a clean retry.
    let token = generate_token();
    let entry = build_token_entry(token.clone(), request_mode);
    let overlay = build_overlay(&entry);

    let config_snippet = match &s.setup_writeback {
        SetupWriteback::Auto(path) => {
            config::to_file(&overlay, path).map_err(|e| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("write config to {}: {e}", path.display()),
                )
            })?;
            tracing::info!(
                path = %path.display(),
                "wrote setup config — token persisted for next boot"
            );
            None
        }
        SetupWriteback::Explicit => {
            // Operator passed --config; return the snippet for them to
            // paste. The token is still live in this process via the
            // hot-swap below.
            let snippet = serde_yml::to_string(&overlay).map_err(|e| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("serialize config snippet: {e}"),
                )
            })?;
            tracing::warn!(
                "setup completed in --config mode — operator must paste the returned \
                 `config_snippet` into their config file manually"
            );
            Some(snippet)
        }
        SetupWriteback::Disabled => {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "setup is not enabled on this role",
            ));
        }
    };

    // Hot-swap the provider. After this atomic store, every
    // subsequent request sees the new `StaticTokenProvider`. Requests
    // currently in flight that already resolved `AuthContext::dev_null`
    // (empty table case) or failed the empty table complete on the
    // old provider — that's fine; they're one-shot resolves.
    let provider = Arc::new(StaticTokenProvider::new(std::iter::once(entry)));
    s.swap_auth_provider(provider);

    // Flip the status slot last. Any concurrent caller waiting on the
    // mutex will read the new value and return 409. Any unauthenticated
    // read of `/agent/setup` outside this flow is gated by the 503
    // middleware, which also notices the status change.
    s.graph
        .write_slot(
            &setup_path,
            "status",
            JsonValue::String("local".to_string()),
        )
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("update /agent/setup.status: {e}"),
            )
        })?;

    // Cloud mode also plants the first tenant + admin user nodes so
    // Phase B can attach Zitadel identity records to them without a
    // schema migration. Phase A has no consumer for these nodes, but
    // creating them eagerly keeps the forward-compat story simple.
    if let SetupRequest::Cloud {
        org_name,
        admin_email,
        ..
    } = &req
    {
        if let Err(e) = seed_tenant_and_admin(&s, org_name, admin_email) {
            tracing::warn!(
                error = %e,
                "cloud setup: failed to seed sys.auth.tenant / sys.auth.user — setup still \
                 succeeded but the Phase-B identity migration will need to run without them"
            );
        }
    }

    tracing::info!(
        mode = request_mode,
        "first-boot setup completed — provider hot-swapped to static_token"
    );

    Ok(Json(SetupResponse {
        status: "ok",
        token,
        advice: "Store this token — it will not be shown again.",
        config_snippet,
    }))
}

// ---- enroll --------------------------------------------------------------

/// Request body for `POST /api/v1/auth/enroll`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollRequest {
    pub cloud_url: String,
    pub enrollment_token: String,
}

async fn enroll(
    State(_s): State<AppState>,
    Json(_req): Json<EnrollRequest>,
) -> Result<Response, ApiError> {
    // Phase A scope gap: the cloud-side `POST /api/v1/agents/enroll`
    // endpoint and the `ZitadelProvider` both land in Phase B. Wiring
    // the route now keeps the client + CLI surfaces stable so they
    // don't need to change when Phase B lands. Until then every
    // enrollment is a no-op that tells the operator why.
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        "edge enrollment requires the Zitadel provider, which lands in Phase B. \
         See docs/design/SYSTEM-BOOTSTRAP.md § Phases.",
    ))
}

// ---- helpers -------------------------------------------------------------

/// 32 random bytes, base64url-unpadded — 256 bits of entropy. URL-safe
/// so the token drops into an `Authorization: Bearer …` header, a
/// JSON body, or a shell argument without extra encoding.
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::rng().fill_bytes(&mut buf);
    // Minimal base64url impl — avoids adding a `base64` direct dep for
    // one call site. Table per RFC 4648 §5.
    const A: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(43);
    for chunk in buf.chunks(3) {
        match chunk.len() {
            3 => {
                let b = u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8 | u32::from(chunk[2]);
                out.push(A[((b >> 18) & 63) as usize] as char);
                out.push(A[((b >> 12) & 63) as usize] as char);
                out.push(A[((b >> 6) & 63) as usize] as char);
                out.push(A[(b & 63) as usize] as char);
            }
            2 => {
                let b = u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8;
                out.push(A[((b >> 18) & 63) as usize] as char);
                out.push(A[((b >> 12) & 63) as usize] as char);
                out.push(A[((b >> 6) & 63) as usize] as char);
            }
            1 => {
                let b = u32::from(chunk[0]) << 16;
                out.push(A[((b >> 18) & 63) as usize] as char);
                out.push(A[((b >> 12) & 63) as usize] as char);
            }
            _ => unreachable!(),
        }
    }
    out
}

fn build_token_entry(token: String, mode: &str) -> StaticTokenEntry {
    let label = format!("setup-admin-{mode}");
    StaticTokenEntry {
        token,
        actor: Actor::Machine {
            id: NodeId::new(),
            label,
        },
        tenant: TenantId::default_tenant(),
        // Admin implies every other scope; the setup token is the
        // break-glass credential for the first operator.
        scopes: vec![Scope::Admin],
    }
}

fn build_overlay(entry: &StaticTokenEntry) -> AgentConfigOverlay {
    AgentConfigOverlay {
        auth: Some(AuthOverlay::StaticToken(StaticTokenOverlay {
            tokens: vec![entry.clone()],
        })),
        ..AgentConfigOverlay::default()
    }
}

fn setup_node_path() -> NodePath {
    NodePath::from_str("/agent/setup").expect("literal path")
}

fn read_status(s: &AppState, path: &NodePath) -> Result<String, ApiError> {
    read_string_slot(s, path, "status")
}

fn read_mode(s: &AppState, path: &NodePath) -> Result<String, ApiError> {
    read_string_slot(s, path, "mode")
}

fn read_string_slot(s: &AppState, path: &NodePath, slot: &str) -> Result<String, ApiError> {
    let snap = s
        .graph
        .get(path)
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "setup node missing"))?;
    let (_, val) = snap
        .slot_values
        .iter()
        .find(|(n, _)| n == slot)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("slot `{slot}` not seeded on /agent/setup"),
            )
        })?;
    val.value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("slot `{slot}` on /agent/setup is not a string"),
        ))
}

fn seed_tenant_and_admin(
    s: &AppState,
    org_name: &str,
    admin_email: &str,
) -> Result<(), graph::GraphError> {
    use spi::KindId;
    let root = NodePath::root();
    let tenant_slug = slugify(org_name);
    let tenant_path = root.child(&tenant_slug);
    if s.graph.get(&tenant_path).is_none() {
        s.graph
            .create_child(&root, KindId::new("sys.auth.tenant"), &tenant_slug)?;
        s.graph
            .write_slot(
                &tenant_path,
                "display_name",
                JsonValue::String(org_name.to_string()),
            )?;
    }
    let user_slug = slugify(admin_email.split('@').next().unwrap_or(admin_email));
    let user_path = tenant_path.child(&user_slug);
    if s.graph.get(&user_path).is_none() {
        s.graph
            .create_child(&tenant_path, KindId::new("sys.auth.user"), &user_slug)?;
    }
    Ok(())
}

/// Lowercase + replace non-alphanumerics with `-`. Deliberately simple
/// — the user-facing display name is preserved in `display_name`; the
/// slug only has to be a valid NodePath segment.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "org".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_43_char_base64url_token() {
        let t = generate_token();
        assert_eq!(t.len(), 43, "32 random bytes → 43 chars of base64url");
        assert!(
            t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "token contains non-base64url char: {t:?}"
        );
    }

    #[test]
    fn generate_token_is_not_predictable() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b, "two successive tokens should differ");
    }

    #[test]
    fn slugify_strips_punctuation() {
        assert_eq!(slugify("Acme Corp"), "acme-corp");
        assert_eq!(slugify("admin@example.com"), "admin-example-com");
        assert_eq!(slugify("!!!"), "org");
    }
}
