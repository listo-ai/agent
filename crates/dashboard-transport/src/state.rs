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
use crate::widget_registry::WidgetRegistry;

#[derive(Clone)]
pub struct DashboardState {
    pub reader: Arc<dyn NodeReader + Send + Sync + 'static>,
    pub widgets: Arc<WidgetRegistry>,
    pub acl: Arc<dyn AclCheck>,
    pub audit: Arc<dyn AuditSink>,
}

impl DashboardState {
    pub fn new(reader: Arc<dyn NodeReader + Send + Sync + 'static>) -> Self {
        Self {
            reader,
            widgets: Arc::new(WidgetRegistry::new()),
            acl: Arc::new(AllowAll),
            audit: Arc::new(TracingAudit),
        }
    }

    pub fn with_widgets(mut self, widgets: Arc<WidgetRegistry>) -> Self {
        self.widgets = widgets;
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
}
