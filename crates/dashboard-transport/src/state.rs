//! Router state — handles threaded through every handler.
//!
//! The reader is the only graph surface the dashboard layer needs; we
//! deliberately don't reach for `AppState` to keep this crate decoupled
//! from `transport-rest`. Call sites (agent bootstrap) wrap the graph
//! store with [`crate::GraphReader`] and hand over the `Arc<dyn
//! NodeReader + Send + Sync>`.

use std::sync::Arc;

use dashboard_runtime::NodeReader;

use crate::acl::{AclCheck, AllowAll};
use crate::audit::{AuditSink, TracingAudit};
use crate::handler_registry::HandlerRegistry;
use crate::invalidate::{InvalidateSink, TracingInvalidate};
use crate::widget_registry::WidgetRegistry;

#[derive(Clone)]
pub struct DashboardState {
    pub reader: Arc<dyn NodeReader + Send + Sync + 'static>,
    pub widgets: Arc<WidgetRegistry>,
    pub handlers: Arc<HandlerRegistry>,
    pub acl: Arc<dyn AclCheck>,
    pub audit: Arc<dyn AuditSink>,
    pub invalidate: Arc<dyn InvalidateSink>,
}

impl DashboardState {
    pub fn new(reader: Arc<dyn NodeReader + Send + Sync + 'static>) -> Self {
        Self {
            reader,
            widgets: Arc::new(WidgetRegistry::new()),
            handlers: Arc::new(HandlerRegistry::new()),
            acl: Arc::new(AllowAll),
            audit: Arc::new(TracingAudit),
            invalidate: Arc::new(TracingInvalidate),
        }
    }

    pub fn with_widgets(mut self, widgets: Arc<WidgetRegistry>) -> Self {
        self.widgets = widgets;
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
