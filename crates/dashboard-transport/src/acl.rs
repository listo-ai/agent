//! ACL check seam.
//!
//! Real RBAC integration lands with the node-model's ACL plumbing; this
//! trait keeps the dashboard layer honest about where the call happens
//! (per-widget, before emission) and lets tests inject deny rules.
//!
//! See DASHBOARD.md § "ACL policy" — per-widget redaction, not
//! page-level 403.

use spi::NodeId;

/// Subject identity passed to ACL checks. We deliberately stay loose on
/// the shape until the real `AuthContext` is threaded through the
/// transport layer; for now an opaque subject string is sufficient to
/// exercise the branching.
#[derive(Debug, Clone, Copy)]
pub struct AclSubject<'a> {
    pub subject: Option<&'a str>,
}

pub trait AclCheck: Send + Sync {
    fn can_read(&self, subject: AclSubject<'_>, node: &NodeId) -> bool;
}

/// Default — every caller can read every node. Production deployments
/// swap this for a real implementation that consults the node model.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAll;

impl AclCheck for AllowAll {
    fn can_read(&self, _subject: AclSubject<'_>, _node: &NodeId) -> bool {
        true
    }
}

/// Test helper — denies read on a fixed set of node ids.
#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Default, Clone)]
pub struct DenyNodes {
    denied: std::collections::HashSet<NodeId>,
}

#[cfg(any(test, feature = "test-helpers"))]
impl DenyNodes {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn deny(mut self, id: NodeId) -> Self {
        self.denied.insert(id);
        self
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl AclCheck for DenyNodes {
    fn can_read(&self, _subject: AclSubject<'_>, node: &NodeId) -> bool {
        !self.denied.contains(node)
    }
}
