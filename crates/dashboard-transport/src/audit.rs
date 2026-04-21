//! Audit events emitted by the resolver.
//!
//! DASHBOARD.md calls for an audit event per widget redaction:
//! `(widgetId, boundNodeId, authSubject)`. Until a first-class audit
//! crate lands, events flow through this trait — the default
//! implementation writes to `tracing` so operators see them in the
//! daemon log.

use spi::NodeId;

#[derive(Debug, Clone)]
pub enum AuditEvent<'a> {
    WidgetRedacted {
        widget: NodeId,
        bound_node: NodeId,
        subject: Option<&'a str>,
    },
    WidgetDangling {
        widget: NodeId,
        missing_node: NodeId,
        subject: Option<&'a str>,
    },
    UnknownWidgetType {
        widget: NodeId,
        widget_type: &'a str,
        subject: Option<&'a str>,
    },
    /// A two-way bound control was stripped from the write plan because
    /// the caller lacks write permission on the target slot. The
    /// component remains visible (read ACL is unaffected); only the
    /// write entry is dropped. One event per redacted component.
    WriteRedacted {
        /// IR component id of the control (`toggle` / `slider`).
        component_id: &'a str,
        /// Absolute path of the node whose slot was redacted.
        path: &'a str,
        /// Slot name that was write-denied.
        slot: &'a str,
        subject: Option<&'a str>,
    },
}

pub trait AuditSink: Send + Sync {
    fn emit(&self, event: AuditEvent<'_>);
}

/// Default — logs events at INFO through `tracing`.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingAudit;

impl AuditSink for TracingAudit {
    fn emit(&self, event: AuditEvent<'_>) {
        match event {
            AuditEvent::WidgetRedacted {
                widget,
                bound_node,
                subject,
            } => tracing::info!(
                target: "dashboard.audit",
                widget = %widget,
                bound_node = %bound_node,
                subject = subject.unwrap_or("<anon>"),
                "widget redacted (acl)",
            ),
            AuditEvent::WidgetDangling {
                widget,
                missing_node,
                subject,
            } => tracing::info!(
                target: "dashboard.audit",
                widget = %widget,
                missing_node = %missing_node,
                subject = subject.unwrap_or("<anon>"),
                "widget dangling (bound node missing)",
            ),
            AuditEvent::UnknownWidgetType {
                widget,
                widget_type,
                subject,
            } => tracing::info!(
                target: "dashboard.audit",
                widget = %widget,
                widget_type = widget_type,
                subject = subject.unwrap_or("<anon>"),
                "widget has unknown type",
            ),
            AuditEvent::WriteRedacted {
                component_id,
                path,
                slot,
                subject,
            } => tracing::info!(
                target: "dashboard.audit",
                component_id = component_id,
                path = path,
                slot = slot,
                subject = subject.unwrap_or("<anon>"),
                "write-plan entry redacted (acl write-deny)",
            ),
        }
    }
}

/// Test helper — records every event in a mutex-protected Vec.
#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Default)]
pub struct RecordingAudit {
    events: std::sync::Mutex<Vec<OwnedAuditEvent>>,
}

#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedAuditEvent {
    WidgetRedacted {
        widget: NodeId,
        bound_node: NodeId,
        subject: Option<String>,
    },
    WidgetDangling {
        widget: NodeId,
        missing_node: NodeId,
        subject: Option<String>,
    },
    UnknownWidgetType {
        widget: NodeId,
        widget_type: String,
        subject: Option<String>,
    },
    WriteRedacted {
        component_id: String,
        path: String,
        slot: String,
        subject: Option<String>,
    },
}

#[cfg(any(test, feature = "test-helpers"))]
impl RecordingAudit {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> Vec<OwnedAuditEvent> {
        self.events.lock().expect("RecordingAudit poisoned").clone()
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl AuditSink for RecordingAudit {
    fn emit(&self, event: AuditEvent<'_>) {
        let owned = match event {
            AuditEvent::WidgetRedacted {
                widget,
                bound_node,
                subject,
            } => OwnedAuditEvent::WidgetRedacted {
                widget,
                bound_node,
                subject: subject.map(String::from),
            },
            AuditEvent::WidgetDangling {
                widget,
                missing_node,
                subject,
            } => OwnedAuditEvent::WidgetDangling {
                widget,
                missing_node,
                subject: subject.map(String::from),
            },
            AuditEvent::UnknownWidgetType {
                widget,
                widget_type,
                subject,
            } => OwnedAuditEvent::UnknownWidgetType {
                widget,
                widget_type: widget_type.to_string(),
                subject: subject.map(String::from),
            },
            AuditEvent::WriteRedacted {
                component_id,
                path,
                slot,
                subject,
            } => OwnedAuditEvent::WriteRedacted {
                component_id: component_id.to_string(),
                path: path.to_string(),
                slot: slot.to_string(),
                subject: subject.map(String::from),
            },
        };
        self.events
            .lock()
            .expect("RecordingAudit poisoned")
            .push(owned);
    }
}
