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
        }
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
