//! Server-Driven UI — Component IR.
//!
//! This crate defines the typed component tree emitted by the backend
//! and rendered by the React runtime. Every component is a variant of
//! [`Component`] discriminated by a stable `"type"` field on the wire.
//!
//! See `docs/design/SDUI.md` for the full design.

mod component;

pub use component::{Action, Component, DiffAnnotation, Tab, TableColumn, TableSource};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// IR version stamped at the root of every tree. The client advertises
/// supported versions in the capability handshake; the server clamps
/// emission to the highest mutually-supported version. Adding a
/// component variant is a minor bump; removing or re-shaping is a
/// major bump with a 12-month deprecation window.
pub const IR_VERSION: u32 = 1;

/// Root of every component tree. Carries the IR version so clients can
/// refuse to render incompatible trees.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ComponentTree {
    /// Protocol version — currently [`IR_VERSION`].
    pub ir_version: u32,
    /// The root component (always a `page` variant for resolve output).
    pub root: Component,
}

impl ComponentTree {
    /// Build a tree with the current [`IR_VERSION`].
    pub fn new(root: Component) -> Self {
        Self {
            ir_version: IR_VERSION,
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_minimal_tree() {
        let tree = ComponentTree::new(Component::Page {
            id: "p1".into(),
            title: Some("Hello".into()),
            children: vec![],
        });
        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json["ir_version"], 1);
        assert_eq!(json["root"]["type"], "page");
        assert_eq!(json["root"]["title"], "Hello");

        let back: ComponentTree = serde_json::from_value(json).unwrap();
        assert_eq!(back.ir_version, 1);
    }

    #[test]
    fn round_trip_nested_tree() {
        let tree = ComponentTree::new(Component::Page {
            id: "p1".into(),
            title: Some("Test".into()),
            children: vec![Component::Col {
                id: None,
                children: vec![
                    Component::Text {
                        id: Some("t1".into()),
                        content: "Hello".into(),
                        intent: None,
                    },
                    Component::Button {
                        id: Some("b1".into()),
                        label: "Click".into(),
                        intent: None,
                        disabled: None,
                        action: Some(Action {
                            handler: "do_thing".into(),
                            args: None,
                        }),
                    },
                ],
                gap: None,
            }],
        });
        let json = serde_json::to_string(&tree).unwrap();
        let back: ComponentTree = serde_json::from_str(&json).unwrap();
        match &back.root {
            Component::Page { children, .. } => assert_eq!(children.len(), 1),
            other => panic!("expected Page, got {other:?}"),
        }
    }

    #[test]
    fn json_schema_emits() {
        let schema = schemars::schema_for!(ComponentTree);
        let json = serde_json::to_string_pretty(&schema).unwrap();
        assert!(json.contains("ComponentTree"));
        assert!(json.contains("ir_version"));
    }
}
