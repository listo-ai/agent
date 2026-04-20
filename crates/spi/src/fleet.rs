//! Fleet transport contract — the trait every backend (NATS, Zenoh,
//! MQTT) implements and every caller (audit stream, command receiver,
//! telemetry pump, graph event mirror) depends on.
//!
//! See `docs/design/FLEET-TRANSPORT.md`. This module owns the *shape*
//! only: types and the trait. The backend crates
//! (`transport-fleet-nats`, `transport-fleet-zenoh`, …) each provide a
//! concrete `FleetTransport` behind a Cargo feature.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthError, TenantId};
use crate::subject::Subject;

/// Raw message bytes on the wire. Alias today; may grow into
/// `bytes::Bytes` later without breaking callers.
pub type Payload = Vec<u8>;

/// One inbound message on a subscription. Handlers can use the subject
/// to distinguish messages when a single subscription covers a wildcard
/// pattern.
#[derive(Debug, Clone)]
pub struct FleetMessage {
    pub subject: Subject,
    pub payload: Payload,
    pub reply_to: Option<Subject>,
}

/// Connection health snapshot. Mirrored into the `sys.agent.fleet`
/// node's `status.connection` slot so flows can react to fleet
/// lifecycle as a first-class event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Transport is up and exchanging messages.
    Connected,
    /// Disconnected, backend is retrying with backoff.
    Reconnecting,
    /// Disconnected with no active retry (e.g. credential rejection).
    Disconnected,
    /// `fleet: null` — standalone mode, transport never constructed.
    Disabled,
}

/// Structured transport failure. Serialisable so cross-process callers
/// (blocks via IPC, Studio via API) see the same shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum FleetError {
    #[error("transport disabled (fleet: null)")]
    Disabled,
    #[error("not connected")]
    NotConnected,
    #[error("timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    #[error("payload exceeds backend limit ({limit_bytes} bytes)")]
    PayloadTooLarge { limit_bytes: u64 },
    #[error("subject rejected: {reason}")]
    InvalidSubject { reason: String },
    /// Message-level auth (the bearer token carried in the headers
    /// frame) failed — distinct from connection-level auth which is
    /// owned by the backend and reported as `Backend`.
    #[error(transparent)]
    Auth(#[from] AuthError),
    /// Whatever the backend reported verbatim. Opaque; callers handle
    /// generically or log-and-fail.
    #[error("backend: {0}")]
    Backend(String),
}

/// Stream of inbound messages on a subscription. Opaque to callers —
/// the concrete stream type is backend-specific; the alias keeps the
/// trait object-safe.
pub type SubscriptionStream =
    Pin<Box<dyn futures_core::Stream<Item = FleetMessage> + Send + 'static>>;

/// Stream of connection-health transitions. Always starts with the
/// current state so subscribers don't need a separate "get current"
/// call to initialise.
pub type HealthStream = Pin<Box<dyn futures_core::Stream<Item = HealthStatus> + Send + 'static>>;

/// A registered server returned from `serve`. Dropping it deregisters
/// the handler from the transport via `ServerHandle::shutdown`.
pub struct Server {
    inner: Box<dyn ServerHandle>,
}

impl Server {
    pub fn new<H: ServerHandle + 'static>(inner: H) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.inner.shutdown();
    }
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server").finish()
    }
}

/// Backend hook for the opaque `Server` handle. Each backend implements
/// this on whatever resource represents an active subscription +
/// handler dispatcher.
pub trait ServerHandle: Send + Sync {
    /// Best-effort graceful shutdown. Default is a no-op; backends that
    /// hold a task JoinHandle override to abort it.
    fn shutdown(&mut self) {}
}

/// One handler for request/reply traffic on a subject pattern.
///
/// Object-safe by design: `FleetTransport::serve` takes `Arc<dyn
/// FleetHandler>` so the same value can live in both the axum state and
/// the fleet subscription dispatcher. One handler fn, two surfaces.
pub trait FleetHandler: Send + Sync {
    /// Handle an inbound message; optionally return reply bytes.
    ///
    /// Implementations typically: deserialise `payload`, call into the
    /// core logic shared with the HTTP route, serialise the result.
    fn handle<'a>(
        &'a self,
        msg: FleetMessage,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Payload>, FleetError>> + Send + 'a>>;
}

/// Which agent an operation is targeting.
///
/// Carried by `AgentClient` (TypeScript), the CLI, and flow node inputs.
/// Dispatch-time routing only — never written to the graph or database.
///
/// Serialises as a tagged union (`{ "kind": "local" }` /
/// `{ "kind": "remote", "tenant": "…", "agent_id": "…" }`) so the
/// TypeScript client can mirror it as a discriminated union with zero
/// hand-rolling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FleetScope {
    /// Talk to the local agent process over HTTP — no fleet required.
    Local,
    /// Issue a fleet request/reply to a specific remote agent.
    Remote {
        tenant: TenantId,
        /// Raw agent identifier, e.g. `"edge-42"`. Escaped by
        /// `SubjectToken::encode` when stamped onto a `Subject`.
        agent_id: String,
    },
}

impl FleetScope {
    /// Build the subject prefix for a given kind segment chain, or `None`
    /// when the scope is `Local` (hits the axum router directly).
    pub fn subject(&self, kind: &str) -> Option<Subject> {
        match self {
            FleetScope::Local => None,
            FleetScope::Remote { tenant, agent_id } => {
                Some(Subject::for_agent(tenant, agent_id).kind(kind).build())
            }
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, FleetScope::Local)
    }
}

/// The fleet transport itself.
///
/// Backends implement this behind a Cargo feature. `AppState` holds an
/// `Arc<dyn FleetTransport>`; `fleet: null` uses a `NullTransport`
/// impl that returns `FleetError::Disabled` on every call except
/// `health()` which yields `HealthStatus::Disabled`.
#[async_trait]
pub trait FleetTransport: Send + Sync {
    /// Fire a one-way message. Delivery semantics per the backend's
    /// `fleet.<backend>.v1` contract.
    async fn publish(&self, subject: &Subject, payload: Payload) -> Result<(), FleetError>;

    /// Request/reply with a bounded timeout.
    async fn request(
        &self,
        subject: &Subject,
        payload: Payload,
        timeout: Duration,
    ) -> Result<Payload, FleetError>;

    /// Subscribe to a subject pattern. Wildcards follow the backend's
    /// native syntax (NATS `*`/`>`, Zenoh `*`/`**`).
    async fn subscribe(&self, pattern: &Subject) -> Result<SubscriptionStream, FleetError>;

    /// Register a request handler on a subject pattern. Symmetric with
    /// `routes::mount` in `transport-rest` — same handlers serve HTTP
    /// and fleet callers.
    async fn serve(
        &self,
        pattern: &Subject,
        handler: std::sync::Arc<dyn FleetHandler>,
    ) -> Result<Server, FleetError>;

    /// Connection state as a stream.
    fn health(&self) -> HealthStream;

    /// Backend id for capability reporting (e.g. `"nats"`, `"zenoh"`,
    /// `"null"`). Surfaces as `fleet.<id>.v1` in `GET /api/v1/capabilities`.
    fn id(&self) -> &'static str;
}

/// The "no-op" fleet transport — used when config is `fleet: null`.
/// Exists here rather than in a backend crate so every app can
/// construct one without conditional compilation.
pub struct NullTransport;

#[async_trait]
impl FleetTransport for NullTransport {
    async fn publish(&self, _s: &Subject, _p: Payload) -> Result<(), FleetError> {
        Err(FleetError::Disabled)
    }

    async fn request(
        &self,
        _s: &Subject,
        _p: Payload,
        _t: Duration,
    ) -> Result<Payload, FleetError> {
        Err(FleetError::Disabled)
    }

    async fn subscribe(&self, _p: &Subject) -> Result<SubscriptionStream, FleetError> {
        Err(FleetError::Disabled)
    }

    async fn serve(
        &self,
        _p: &Subject,
        _h: std::sync::Arc<dyn FleetHandler>,
    ) -> Result<Server, FleetError> {
        Err(FleetError::Disabled)
    }

    fn health(&self) -> HealthStream {
        Box::pin(DisabledHealthStream::new())
    }

    fn id(&self) -> &'static str {
        "null"
    }
}

/// Single-item stream that yields `Disabled` once and ends. Inlined so
/// `spi` doesn't need to pull `futures-util`.
struct DisabledHealthStream(bool);

impl DisabledHealthStream {
    fn new() -> Self {
        Self(false)
    }
}

impl futures_core::Stream for DisabledHealthStream {
    type Item = HealthStatus;
    fn poll_next(
        mut self: Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.0 {
            std::task::Poll::Ready(None)
        } else {
            self.0 = true;
            std::task::Poll::Ready(Some(HealthStatus::Disabled))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_transport_reports_disabled() {
        let t = NullTransport;
        let subj = Subject::for_agent(&crate::TenantId::default_tenant(), "edge-1")
            .kind("cmd.ping")
            .build();
        let err = t.publish(&subj, vec![]).await.unwrap_err();
        assert!(matches!(err, FleetError::Disabled));
        assert_eq!(t.id(), "null");
    }

    #[test]
    fn fleet_error_serde_round_trips() {
        let e = FleetError::Timeout { timeout_ms: 500 };
        let s = serde_json::to_string(&e).unwrap();
        let back: FleetError = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }

    /// `FleetScope` JSON contract is shared with the TypeScript client.
    /// The shape must match `FleetScopeSchema` in `clients/ts/src/schemas/fleet.ts`.
    #[test]
    fn fleet_scope_serde_contract() {
        // Local → `{"kind":"local"}`
        let local = FleetScope::Local;
        let json = serde_json::to_string(&local).unwrap();
        assert_eq!(json, r#"{"kind":"local"}"#);
        let back: FleetScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, local);

        // Remote → `{"kind":"remote","tenant":"sys","agent_id":"edge-42"}`
        let remote = FleetScope::Remote {
            tenant: crate::TenantId::new("sys"),
            agent_id: "edge-42".to_string(),
        };
        let json = serde_json::to_string(&remote).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"remote","tenant":"sys","agent_id":"edge-42"}"#
        );
        let back: FleetScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, remote);
    }

    #[test]
    fn fleet_scope_subject_returns_none_for_local() {
        assert!(FleetScope::Local.subject("api.v1.nodes.list").is_none());
    }

    #[test]
    fn fleet_scope_subject_builds_canonical_form_for_remote() {
        let s = FleetScope::Remote {
            tenant: crate::TenantId::new("sys"),
            agent_id: "edge-42".to_string(),
        }
        .subject("api.v1.nodes.list")
        .unwrap();
        assert_eq!(s.as_dotted(), "fleet.sys.edge-42.api.v1.nodes.list");
    }

    /// Compile-time check: trait is object-safe so `Arc<dyn FleetTransport>`
    /// works for shared state.
    #[allow(dead_code)]
    fn _assert_object_safe(_t: &dyn FleetTransport) {}
}
