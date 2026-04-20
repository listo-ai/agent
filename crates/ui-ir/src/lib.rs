//! Server-Driven UI — Component IR.
//!
//! This crate defines the typed component tree emitted by the backend
//! and rendered by the React runtime. Every component is a variant of
//! [`Component`] discriminated by a stable `"type"` field on the wire.
//!
//! See `docs/design/SDUI.md` for the full design.

mod component;

pub use component::{
    Action, Component, DateRangePreset, DiffAnnotation, SelectOption, Tab, TableColumn,
    TableSource,
};

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
    /// Author-declared constants, referenced from bindings via
    /// `{{$vars.<key>}}`. Scoped to the whole tree; resolved once per
    /// resolve call, before any other binding substitution. Values are
    /// any JSON — strings, numbers, arrays, nested objects. Vars
    /// cannot reference other vars in v1 (no recursion).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub vars: std::collections::HashMap<String, serde_json::Value>,
}

impl ComponentTree {
    /// Build a tree with the current [`IR_VERSION`].
    pub fn new(root: Component) -> Self {
        Self {
            ir_version: IR_VERSION,
            root,
            vars: std::collections::HashMap::new(),
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
                            optimistic: None,
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
