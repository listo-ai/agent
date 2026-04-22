//! First-boot setup orchestration.
//!
//! [`SetupService`] is the one domain-layer object that implements the
//! state machine described in `docs/design/SYSTEM-BOOTSTRAP.md`:
//!
//! ```text
//!  unconfigured ──(POST /auth/setup)──► local ──(POST /auth/enroll)──► cloud_enrolled
//! ```
//!
//! Every transport surface (REST today; gRPC, CLI, fleet in future)
//! calls [`SetupService::complete_local`] to drive the first
//! transition. The service owns all of:
//!
//! - the check-then-act single-flight (`tokio::Mutex`) — so two
//!   concurrent setup calls can't both generate a token;
//! - the graph reads for `/agent/setup.{status,mode}`;
//! - the random-token generation (256 bits via `OsRng`);
//! - the [`auth::ProviderCell`] hot-swap;
//! - the config-file writeback (delegates to [`config::to_file`]);
//! - the optional cloud-mode seeding of `sys.auth.tenant` /
//!   `sys.auth.user` nodes for forward-compat with Phase B.
//!
//! The service is transport-agnostic. Transport handlers are responsible
//! only for (a) extracting inputs, (b) calling the service, and (c)
//! serialising the outcome to the wire. This matches the
//! `HOW-TO-ADD-CODE.md` Rule I constraint and means a CLI / fleet
//! surface can adopt setup without duplicating any logic.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use auth::{ProviderCell, StaticTokenEntry, StaticTokenProvider};
use config::{AgentConfigOverlay, AuthOverlay, StaticTokenOverlay};
use graph::GraphStore;
use rand::RngCore;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use spi::{Actor, KindId, NodeId, NodePath, Scope, TenantId};
use tokio::sync::Mutex;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Role mode claimed by the caller. Must match the agent's
/// `/agent/setup.mode` slot or [`SetupService::complete_local`] rejects
/// with [`SetupError::ModeMismatch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    Cloud,
    Edge,
    Standalone,
}

impl SetupMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Edge => "edge",
            Self::Standalone => "standalone",
        }
    }
}

/// Optional cloud-mode metadata for `sys.auth.tenant` +
/// `sys.auth.user` seeding. `None` skips the seed (edge / standalone).
#[derive(Debug, Clone)]
pub struct OrgInfo {
    pub org_name: String,
    pub admin_email: String,
}

/// How the generated `StaticToken` entry should be persisted. Set
/// once at boot by the composition root (`apps/agent/main.rs`) based
/// on whether `--config <path>` was passed.
#[derive(Debug, Clone)]
pub enum SetupWriteback {
    /// Write an `agent.yaml` at this path with mode `0600`. Used when
    /// the operator did not pass `--config` — the default shipping
    /// location is next to `agent.db`.
    Auto(PathBuf),
    /// Operator launched with `--config <path>`. The service refuses
    /// to rewrite their config file (serde_yml does not preserve
    /// comments or key order); [`SetupOutcome::config_snippet`] is
    /// returned instead for the operator to paste. The token is live
    /// in-process for the current run.
    Explicit,
}

/// Outcome of a successful [`SetupService::complete_local`] call.
#[derive(Debug)]
pub struct SetupOutcome {
    /// Bearer token the caller should save and send as
    /// `Authorization: Bearer <token>` for subsequent requests.
    pub token: String,
    /// Only populated for [`SetupWriteback::Explicit`]. Callers
    /// surface this to the operator alongside `token`.
    pub config_snippet: Option<String>,
}

/// Domain-layer errors. Transports map to their status codes.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    /// `/agent/setup.status` is not `"unconfigured"`. Maps to `409`.
    #[error("already configured (status=`{actual}`)")]
    AlreadyConfigured { actual: String },
    /// Request `mode` did not match the agent's seeded `/agent/setup.mode`.
    /// Maps to `400`.
    #[error("mode_mismatch: agent started in `{expected}`, request claimed `{requested}`")]
    ModeMismatch {
        expected: String,
        requested: String,
    },
    /// `/agent/setup` node is missing — service was attached but never
    /// seeded, or the seed was deleted out-of-band. Maps to `503`.
    #[error("setup node missing at {0}")]
    SetupNodeMissing(NodePath),
    /// `/agent/setup.<slot>` is missing or not a string. Indicates
    /// the manifest and the seeding code have drifted. Maps to `500`.
    #[error("setup slot `{0}` is missing or malformed")]
    SlotShape(&'static str),
    /// Graph write failed. Typically a storage-backend problem.
    #[error("graph write: {0}")]
    Graph(#[from] graph::GraphError),
    /// Config write-back failed. See [`config::WriteBackError`].
    #[error("config write-back: {0}")]
    WriteBack(#[from] config::WriteBackError),
    /// YAML serialisation failed during snippet generation.
    #[error("serialize config snippet: {0}")]
    Serialize(#[source] serde_yml::Error),
}

// ── Service ───────────────────────────────────────────────────────────────────

/// Orchestrator for first-boot setup. Clone-friendly (internal
/// state is `Arc`-wrapped) so transports can stash a clone in their
/// state struct without dealing with lifetimes.
#[derive(Clone)]
pub struct SetupService {
    inner: Arc<Inner>,
}

struct Inner {
    graph: Arc<GraphStore>,
    provider_cell: ProviderCell,
    writeback: SetupWriteback,
    /// Serialises the check-then-act sequence in
    /// [`SetupService::complete_local`]. The first caller takes the
    /// lock, reads status, generates, writes, swaps, releases. The
    /// second caller waits, sees `status != "unconfigured"`, returns
    /// [`SetupError::AlreadyConfigured`] — never a second token.
    guard: Mutex<()>,
}

impl SetupService {
    /// Construct. Does not touch the graph — call [`seed`](Self::seed)
    /// separately from the composition root so the graph is seeded
    /// before the router mounts.
    pub fn new(
        graph: Arc<GraphStore>,
        provider_cell: ProviderCell,
        writeback: SetupWriteback,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                graph,
                provider_cell,
                writeback,
                guard: Mutex::new(()),
            }),
        }
    }

    /// Idempotently create `/agent/setup` (`sys.auth.setup`) under
    /// `/agent` and seed its slots. `mode` drives the `mode` slot:
    /// `"standalone"` | `"edge"` | `"cloud"`.
    ///
    /// On a fresh seed `status` starts as `"unconfigured"`. On a
    /// restart where setup completed, the existing `status` slot is
    /// preserved (so the 503 gate does not re-engage), but `mode` is
    /// refreshed so a role change between restarts is reflected.
    pub fn seed(&self, mode: SetupMode) -> Result<(), SetupError> {
        let agent_path = NodePath::from_str("/agent").expect("literal path");
        let setup_path = setup_node_path();

        let existed = self.inner.graph.get(&setup_path).is_some();
        if !existed {
            self.inner
                .graph
                .create_child(&agent_path, KindId::new("sys.auth.setup"), "setup")?;
            self.inner.graph.write_slot(
                &setup_path,
                "status",
                JsonValue::String("unconfigured".to_string()),
            )?;
            self.inner
                .graph
                .write_slot(&setup_path, "cloud_url", JsonValue::Null)?;
            self.inner
                .graph
                .write_slot(&setup_path, "enrolled_at", JsonValue::Null)?;
        }
        self.inner.graph.write_slot(
            &setup_path,
            "mode",
            JsonValue::String(mode.as_str().to_string()),
        )?;
        tracing::info!(mode = mode.as_str(), seeded = !existed, "seeded /agent/setup");
        Ok(())
    }

    /// Read `/agent/setup.status`. Transports use this for the 503
    /// gate and for the whoami-style surfaces that want to report
    /// setup state.
    pub fn status(&self) -> Result<String, SetupError> {
        read_string_slot(&self.inner.graph, &setup_node_path(), "status")
    }

    /// `true` when `status != "unconfigured"` (`"local"` or
    /// `"cloud_enrolled"`). Convenience wrapper for the 503 middleware.
    pub fn is_configured(&self) -> bool {
        self.status()
            .map(|s| s != "unconfigured")
            .unwrap_or(false)
    }

    /// Run the full first-boot transition: check → generate → write →
    /// swap → flip-status. Single-flight by construction.
    ///
    /// `mode` must match the seeded `/agent/setup.mode` slot (set by
    /// [`Self::seed`]). `org` is required for [`SetupMode::Cloud`] to
    /// seed `sys.auth.tenant` + `sys.auth.user` nodes; ignored for
    /// other modes.
    ///
    /// The returned [`SetupOutcome::token`] is the single source of
    /// truth for the operator. It is also persisted per the service's
    /// [`SetupWriteback`] policy.
    pub async fn complete_local(
        &self,
        mode: SetupMode,
        org: Option<OrgInfo>,
    ) -> Result<SetupOutcome, SetupError> {
        let _guard = self.inner.guard.lock().await;
        let setup_path = setup_node_path();

        // Check phase.
        let status = read_string_slot(&self.inner.graph, &setup_path, "status")?;
        if status != "unconfigured" {
            return Err(SetupError::AlreadyConfigured { actual: status });
        }
        let seeded_mode = read_string_slot(&self.inner.graph, &setup_path, "mode")?;
        if seeded_mode != mode.as_str() {
            return Err(SetupError::ModeMismatch {
                expected: seeded_mode,
                requested: mode.as_str().to_string(),
            });
        }

        // Generate.
        let token = generate_token();
        let entry = build_token_entry(&token, mode);
        let overlay = overlay_with_single_token(&entry);

        // Persist before in-memory state flip, so a persistence
        // failure aborts without leaving a ghost provider that only
        // authenticates this process until restart.
        let config_snippet = match &self.inner.writeback {
            SetupWriteback::Auto(path) => {
                config::to_file(&overlay, path)?;
                tracing::info!(
                    path = %path.display(),
                    "setup: wrote agent.yaml — token persists across restart",
                );
                None
            }
            SetupWriteback::Explicit => {
                let snippet = serde_yml::to_string(&overlay).map_err(SetupError::Serialize)?;
                tracing::warn!(
                    "setup: `--config` mode — operator must paste the returned config_snippet \
                     into their config file for the token to survive a restart",
                );
                Some(snippet)
            }
        };

        // Hot-swap the provider. After this atomic store, every
        // subsequent request authenticates against the new
        // `StaticTokenProvider`.
        let provider = Arc::new(StaticTokenProvider::new(std::iter::once(entry)));
        self.inner.provider_cell.store(provider);

        // Cloud-mode also plants the tenant + admin user nodes so
        // Phase B can attach Zitadel records without a migration.
        if let (SetupMode::Cloud, Some(info)) = (mode, org.as_ref()) {
            if let Err(e) = self.seed_tenant_and_admin(info) {
                tracing::warn!(
                    error = %e,
                    "setup: failed to seed sys.auth.tenant / sys.auth.user — Phase B will \
                     need to run without them"
                );
            }
        }

        // Flip status LAST. A concurrent caller waiting on the mutex
        // observes the new value and gets `AlreadyConfigured`.
        self.inner.graph.write_slot(
            &setup_path,
            "status",
            JsonValue::String("local".to_string()),
        )?;

        tracing::info!(mode = mode.as_str(), "first-boot setup completed");
        Ok(SetupOutcome {
            token,
            config_snippet,
        })
    }

    fn seed_tenant_and_admin(&self, info: &OrgInfo) -> Result<(), graph::GraphError> {
        let root = NodePath::root();
        let tenant_slug = slugify(&info.org_name);
        let tenant_path = root.child(&tenant_slug);
        if self.inner.graph.get(&tenant_path).is_none() {
            self.inner
                .graph
                .create_child(&root, KindId::new("sys.auth.tenant"), &tenant_slug)?;
            self.inner.graph.write_slot(
                &tenant_path,
                "display_name",
                JsonValue::String(info.org_name.clone()),
            )?;
        }
        let user_slug = slugify(info.admin_email.split('@').next().unwrap_or(&info.admin_email));
        let user_path = tenant_path.child(&user_slug);
        if self.inner.graph.get(&user_path).is_none() {
            self.inner
                .graph
                .create_child(&tenant_path, KindId::new("sys.auth.user"), &user_slug)?;
        }
        Ok(())
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Canonical graph path for the setup node. Exposed so transport
/// layers can use it in 503 middleware without string-literal drift.
pub fn setup_node_path() -> NodePath {
    NodePath::from_str("/agent/setup").expect("literal path")
}

fn read_string_slot(
    graph: &GraphStore,
    path: &NodePath,
    slot: &'static str,
) -> Result<String, SetupError> {
    let snap = graph
        .get(path)
        .ok_or_else(|| SetupError::SetupNodeMissing(path.clone()))?;
    let (_, value) = snap
        .slot_values
        .iter()
        .find(|(n, _)| n == slot)
        .ok_or(SetupError::SlotShape(slot))?;
    value
        .value
        .as_str()
        .map(str::to_string)
        .ok_or(SetupError::SlotShape(slot))
}

/// 32 random bytes → 43 chars of base64url (no padding). 256 bits of
/// entropy. URL-safe so the token drops into Authorization headers,
/// JSON bodies, and shell args without extra encoding.
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::rng().fill_bytes(&mut buf);
    base64url_unpadded(&buf)
}

/// Inline RFC 4648 §5 base64url (no padding). Avoids adding a
/// `base64` direct dep for one call site.
fn base64url_unpadded(input: &[u8]) -> String {
    const A: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(input.len() * 4 / 3 + 3);
    for chunk in input.chunks(3) {
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

fn build_token_entry(token: &str, mode: SetupMode) -> StaticTokenEntry {
    let label = format!("setup-admin-{}", mode.as_str());
    StaticTokenEntry {
        token: token.to_string(),
        actor: Actor::Machine {
            id: NodeId::new(),
            label,
        },
        tenant: TenantId::default_tenant(),
        // Admin implies every other scope — this is the break-glass
        // credential for the first operator.
        scopes: vec![Scope::Admin],
    }
}

fn overlay_with_single_token(entry: &StaticTokenEntry) -> AgentConfigOverlay {
    AgentConfigOverlay {
        auth: Some(AuthOverlay::StaticToken(StaticTokenOverlay {
            tokens: vec![entry.clone()],
        })),
        ..AgentConfigOverlay::default()
    }
}

/// Lowercase + replace non-alphanumerics with `-`. Minimal — the
/// user-facing display name is preserved in `display_name`; the slug
/// only has to be a valid `NodePath` segment.
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use auth::DevNullProvider;
    use graph::{seed, GraphStore, KindRegistry, NullSink};

    fn make_service(writeback: SetupWriteback) -> (SetupService, Arc<GraphStore>) {
        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(<crate::SetupNode as blocks_sdk::NodeKind>::manifest());
        kinds.register(<crate::TenantNode as blocks_sdk::NodeKind>::manifest());
        kinds.register(<crate::UserNode as blocks_sdk::NodeKind>::manifest());
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(
                &NodePath::root(),
                KindId::new("sys.core.folder"),
                "agent",
            )
            .unwrap();
        let cell = ProviderCell::new(Arc::new(DevNullProvider::new()));
        let svc = SetupService::new(graph.clone(), cell, writeback);
        (svc, graph)
    }

    #[test]
    fn generate_token_is_43_chars_base64url() {
        let t = generate_token();
        assert_eq!(t.len(), 43);
        assert!(
            t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "non-base64url char in {t:?}"
        );
    }

    #[test]
    fn generate_token_is_not_predictable() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn slugify_strips_punctuation() {
        assert_eq!(slugify("Acme Corp"), "acme-corp");
        assert_eq!(slugify("admin@example.com"), "admin-example-com");
        assert_eq!(slugify("!!!"), "org");
    }

    #[tokio::test]
    async fn seed_writes_status_unconfigured_and_mode() {
        let (svc, graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Edge).unwrap();
        let snap = graph.get(&setup_node_path()).unwrap();
        let status = snap
            .slot_values
            .iter()
            .find(|(n, _)| n == "status")
            .unwrap()
            .1
            .value
            .as_str()
            .unwrap()
            .to_string();
        let mode = snap
            .slot_values
            .iter()
            .find(|(n, _)| n == "mode")
            .unwrap()
            .1
            .value
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(status, "unconfigured");
        assert_eq!(mode, "edge");
        assert!(!svc.is_configured());
    }

    #[tokio::test]
    async fn complete_local_flips_status_and_returns_token() {
        let (svc, _graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Edge).unwrap();
        let outcome = svc.complete_local(SetupMode::Edge, None).await.unwrap();
        assert_eq!(outcome.token.len(), 43);
        assert!(outcome.config_snippet.is_some()); // explicit mode
        assert!(svc.is_configured());
        assert_eq!(svc.status().unwrap(), "local");
    }

    #[tokio::test]
    async fn second_call_gets_already_configured() {
        let (svc, _graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Edge).unwrap();
        let _ = svc.complete_local(SetupMode::Edge, None).await.unwrap();
        let err = svc.complete_local(SetupMode::Edge, None).await.unwrap_err();
        assert!(matches!(err, SetupError::AlreadyConfigured { .. }));
    }

    #[tokio::test]
    async fn mode_mismatch_is_rejected() {
        let (svc, _graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Edge).unwrap();
        let err = svc
            .complete_local(SetupMode::Cloud, None)
            .await
            .unwrap_err();
        assert!(matches!(err, SetupError::ModeMismatch { .. }));
    }

    #[tokio::test]
    async fn hot_swap_replaces_provider() {
        let (svc, _graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Edge).unwrap();
        // Grab a fresh cell the service was built with so we can
        // observe its id after the swap.
        let cell = svc.inner.provider_cell.clone();
        assert_eq!(cell.id(), "dev_null");
        svc.complete_local(SetupMode::Edge, None).await.unwrap();
        assert_eq!(cell.id(), "static_token");
    }

    #[tokio::test]
    async fn concurrent_setup_calls_single_flight() {
        // Two callers race. Exactly one gets a token; the other sees
        // AlreadyConfigured. Both observe the same final state.
        let (svc, _graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Edge).unwrap();
        let a = svc.clone();
        let b = svc.clone();
        let (ra, rb) = tokio::join!(
            a.complete_local(SetupMode::Edge, None),
            b.complete_local(SetupMode::Edge, None),
        );
        let mut wins = 0;
        let mut already = 0;
        for r in [ra, rb] {
            match r {
                Ok(o) => {
                    assert_eq!(o.token.len(), 43);
                    wins += 1;
                }
                Err(SetupError::AlreadyConfigured { .. }) => already += 1,
                Err(e) => panic!("unexpected error: {e:?}"),
            }
        }
        assert_eq!((wins, already), (1, 1), "exactly one winner, one already");
    }

    #[tokio::test]
    async fn auto_mode_writes_agent_yaml_with_0600() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml = tmp.path().join("agent.yaml");
        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(<crate::SetupNode as blocks_sdk::NodeKind>::manifest());
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(
                &NodePath::root(),
                KindId::new("sys.core.folder"),
                "agent",
            )
            .unwrap();
        let cell = ProviderCell::new(Arc::new(DevNullProvider::new()));
        let svc = SetupService::new(graph, cell, SetupWriteback::Auto(yaml.clone()));
        svc.seed(SetupMode::Edge).unwrap();

        let outcome = svc.complete_local(SetupMode::Edge, None).await.unwrap();
        assert!(outcome.config_snippet.is_none()); // auto mode omits snippet
        assert!(yaml.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&yaml).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[tokio::test]
    async fn cloud_mode_seeds_tenant_and_admin_nodes() {
        let (svc, graph) = make_service(SetupWriteback::Explicit);
        svc.seed(SetupMode::Cloud).unwrap();
        let info = OrgInfo {
            org_name: "Acme Corp".to_string(),
            admin_email: "admin@example.com".to_string(),
        };
        svc.complete_local(SetupMode::Cloud, Some(info))
            .await
            .unwrap();
        let tenant = graph.get(&NodePath::root().child("acme-corp"));
        assert!(tenant.is_some(), "sys.auth.tenant node missing");
        let user = graph.get(&NodePath::root().child("acme-corp").child("admin"));
        assert!(user.is_some(), "sys.auth.user node missing");
    }
}
