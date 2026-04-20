//! Router state — handles threaded through every handler.
//!
//! The reader is the only graph surface the dashboard layer needs; we
//! deliberately don't reach for `AppState` to keep this crate decoupled
//! from `transport-rest`. Call sites (agent bootstrap) wrap the graph
//! store with [`crate::GraphReader`] and hand over the `Arc<dyn
//! NodeReader + Send + Sync>`.

use std::sync::Arc;

use dashboard_runtime::NodeReader;
use graph::KindRegistry;

use crate::acl::{AclCheck, AllowAll};
use crate::audit::{AuditSink, TracingAudit};
use crate::handler_registry::HandlerRegistry;
use crate::invalidate::{InvalidateSink, TracingInvalidate};

#[derive(Clone)]
pub struct DashboardState {
    pub reader: Arc<dyn NodeReader + Send + Sync + 'static>,
    pub handlers: Arc<HandlerRegistry>,
    pub acl: Arc<dyn AclCheck>,
    pub audit: Arc<dyn AuditSink>,
    pub invalidate: Arc<dyn InvalidateSink>,
    /// Kind registry used by `/api/v1/ui/render` to resolve a node's
    /// `KindManifest.views`. `None` while the agent bootstrap has not
    /// wired it in — render requests degrade to 503 in that case.
    pub kinds: Option<Arc<KindRegistry>>,
    /// Anthropic API key for `/api/v1/ui/compose`. Populated from the
    /// `ANTHROPIC_API_KEY` env var at agent startup; `None` disables
    /// AI compose with a deterministic `compose_unavailable` error.
    pub ai_api_key: Option<String>,
    /// Override model for compose. `None` uses the crate default.
    pub ai_model: Option<String>,
}

impl DashboardState {
    pub fn new(reader: Arc<dyn NodeReader + Send + Sync + 'static>) -> Self {
        Self {
            reader,
            handlers: Arc::new(HandlerRegistry::new()),
            acl: Arc::new(AllowAll),
            audit: Arc::new(TracingAudit),
            invalidate: Arc::new(TracingInvalidate),
            kinds: None,
            ai_api_key: None,
            ai_model: None,
        }
    }

    pub fn with_ai_api_key(mut self, key: Option<String>) -> Self {
        self.ai_api_key = key;
        self
    }

    pub fn with_ai_model(mut self, model: Option<String>) -> Self {
        self.ai_model = model;
        self
    }

    pub fn with_kinds(mut self, kinds: Arc<KindRegistry>) -> Self {
        self.kinds = Some(kinds);
        self
    }

    pub fn with_handlers(mut self, handlers: Arc<HandlerRegistry>) -> Self {
        self.handlers = handlers;
        self
    }

    pub fn with_acl(mut self, acl: Arc<dyn AclCheck>) -> Self {
        self.acl = acl;
        self
    }

    pub fn with_audit(mut self, audit: Arc<dyn AuditSink>) -> Self {
        self.audit = audit;
        self
    }

    pub fn with_invalidate(mut self, sink: Arc<dyn InvalidateSink>) -> Self {
        self.invalidate = sink;
        self
    }
}
