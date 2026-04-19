//! `CacheKey` derivation.
//!
//! Mirrors DASHBOARD.md § "The resolver". The resolver is referentially
//! transparent given its inputs; the cache key hashes every input that
//! could change the output so a bit-equal key implies a bit-equal
//! render tree.
//!
//! Caches are advisory — a miss costs a resolve, not correctness — so
//! stability of the hash across process restarts matters more than
//! cryptographic strength. Uses `DefaultHasher` for simplicity; swap for
//! a content-addressed hash later if cross-node caching lands.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use spi::NodeId;

use crate::stack::ContextStack;

/// Every input that feeds the resolver. Caller builds this; runtime
/// hashes it. See DASHBOARD.md for field semantics.
#[derive(Debug, Clone)]
pub struct CacheKeyInputs<'a> {
    pub page_ref: NodeId,
    pub page_node_version: u64,
    pub template_node_version: Option<u64>,
    pub widget_node_versions: &'a [(NodeId, u64)],
    pub bound_node_versions: &'a [(NodeId, u64)],
    pub auth_subject: &'a str,
    pub auth_role_epoch: u64,
    pub stack: &'a ContextStack,
    pub page_state_hash: u64,
    pub widget_registry_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CacheKey(pub u64);

impl CacheKeyInputs<'_> {
    pub fn derive(&self) -> CacheKey {
        let mut h = DefaultHasher::new();

        self.page_ref.0.hash(&mut h);
        self.page_node_version.hash(&mut h);
        self.template_node_version.hash(&mut h);

        let mut widgets = self.widget_node_versions.to_vec();
        widgets.sort_by_key(|(id, _)| id.0);
        for (id, v) in &widgets {
            id.0.hash(&mut h);
            v.hash(&mut h);
        }

        let mut bound = self.bound_node_versions.to_vec();
        bound.sort_by_key(|(id, _)| id.0);
        for (id, v) in &bound {
            id.0.hash(&mut h);
            v.hash(&mut h);
        }

        self.auth_subject.hash(&mut h);
        self.auth_role_epoch.hash(&mut h);

        // Stack contributes each frame's alias (may be null) + nodeRef,
        // in walk order. Index-only frames still contribute their
        // position.
        for f in self.stack.frames() {
            f.alias.hash(&mut h);
            f.node_ref.0.hash(&mut h);
        }

        self.page_state_hash.hash(&mut h);
        self.widget_registry_version.hash(&mut h);

        CacheKey(h.finish())
    }
}

/// Utility: hash a `page_state` JSON value into a `u64` suitable for
/// [`CacheKeyInputs::page_state_hash`]. Serialises canonically (sorted
/// object keys) so semantically-equal values produce equal hashes.
pub fn hash_page_state(page_state: &serde_json::Value) -> u64 {
    let mut h = DefaultHasher::new();
    canonical_hash(page_state, &mut h);
    h.finish()
}

fn canonical_hash(v: &serde_json::Value, h: &mut DefaultHasher) {
    match v {
        serde_json::Value::Null => 0u8.hash(h),
        serde_json::Value::Bool(b) => {
            1u8.hash(h);
            b.hash(h);
        }
        serde_json::Value::Number(n) => {
            2u8.hash(h);
            n.to_string().hash(h);
        }
        serde_json::Value::String(s) => {
            3u8.hash(h);
            s.hash(h);
        }
        serde_json::Value::Array(a) => {
            4u8.hash(h);
            a.len().hash(h);
            for item in a {
                canonical_hash(item, h);
            }
        }
        serde_json::Value::Object(m) => {
            5u8.hash(h);
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for k in keys {
                k.hash(h);
                canonical_hash(&m[k], h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::{ContextStack, Frame};
    use serde_json::json;

    fn stack_of(frames: Vec<Frame>) -> ContextStack {
        ContextStack::from_frames(frames)
    }

    #[test]
    fn identical_inputs_produce_identical_key() {
        let page = NodeId::new();
        let widget = NodeId::new();
        let stack = stack_of(vec![]);
        let widget_versions = vec![(widget, 3)];
        let bound_versions: Vec<(NodeId, u64)> = vec![];
        let inputs = CacheKeyInputs {
            page_ref: page,
            page_node_version: 1,
            template_node_version: Some(2),
            widget_node_versions: &widget_versions,
            bound_node_versions: &bound_versions,
            auth_subject: "user:1",
            auth_role_epoch: 7,
            stack: &stack,
            page_state_hash: 0,
            widget_registry_version: 42,
        };
        assert_eq!(inputs.derive(), inputs.derive());
    }

    #[test]
    fn different_page_version_changes_key() {
        let page = NodeId::new();
        let stack = stack_of(vec![]);
        let widgets: Vec<(NodeId, u64)> = vec![];
        let bound: Vec<(NodeId, u64)> = vec![];
        let a = CacheKeyInputs {
            page_ref: page,
            page_node_version: 1,
            template_node_version: None,
            widget_node_versions: &widgets,
            bound_node_versions: &bound,
            auth_subject: "s",
            auth_role_epoch: 0,
            stack: &stack,
            page_state_hash: 0,
            widget_registry_version: 0,
        };
        let b = CacheKeyInputs {
            page_node_version: 2,
            ..a.clone()
        };
        assert_ne!(a.derive(), b.derive());
    }

    #[test]
    fn widget_version_order_insensitive() {
        let page = NodeId::new();
        let w1 = NodeId::new();
        let w2 = NodeId::new();
        let stack = stack_of(vec![]);
        let bound: Vec<(NodeId, u64)> = vec![];
        let ab = vec![(w1, 1), (w2, 2)];
        let ba = vec![(w2, 2), (w1, 1)];
        let make = |list: &[(NodeId, u64)]| CacheKeyInputs {
            page_ref: page,
            page_node_version: 1,
            template_node_version: None,
            widget_node_versions: list,
            bound_node_versions: &bound,
            auth_subject: "s",
            auth_role_epoch: 0,
            stack: &stack,
            page_state_hash: 0,
            widget_registry_version: 0,
        }
        .derive();
        assert_eq!(make(&ab), make(&ba));
    }

    #[test]
    fn stack_order_is_significant() {
        let page = NodeId::new();
        let t1 = NodeId::new();
        let t2 = NodeId::new();
        let widgets: Vec<(NodeId, u64)> = vec![];
        let bound: Vec<(NodeId, u64)> = vec![];
        let stack_a = stack_of(vec![
            Frame {
                alias: Some("a".into()),
                node_ref: t1,
            },
            Frame {
                alias: Some("b".into()),
                node_ref: t2,
            },
        ]);
        let stack_b = stack_of(vec![
            Frame {
                alias: Some("b".into()),
                node_ref: t2,
            },
            Frame {
                alias: Some("a".into()),
                node_ref: t1,
            },
        ]);
        let make = |s: &ContextStack| CacheKeyInputs {
            page_ref: page,
            page_node_version: 1,
            template_node_version: None,
            widget_node_versions: &widgets,
            bound_node_versions: &bound,
            auth_subject: "s",
            auth_role_epoch: 0,
            stack: s,
            page_state_hash: 0,
            widget_registry_version: 0,
        }
        .derive();
        assert_ne!(make(&stack_a), make(&stack_b));
    }

    #[test]
    fn hash_page_state_is_key_order_insensitive() {
        assert_eq!(
            hash_page_state(&json!({ "a": 1, "b": 2 })),
            hash_page_state(&json!({ "b": 2, "a": 1 })),
        );
        assert_ne!(
            hash_page_state(&json!({ "a": 1 })),
            hash_page_state(&json!({ "a": 2 })),
        );
    }
}
