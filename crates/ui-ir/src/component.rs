//! Component enum — the heart of the IR.
//!
//! Six categories plus placeholder stubs for ACL-redacted and
//! dangling widgets. Every variant carries `#[serde(tag = "type")]`
//! so the wire discriminator is the stable `"type"` field.
//!
//! S1 variants (~15): page, row, col, grid, tabs, text, heading,
//! badge, button, form, table, diff, rich_text, forbidden, dangling.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// -------------------------------------------------------------------
// Component
// -------------------------------------------------------------------

/// A single component in the IR tree.
///
/// Discriminated by the `"type"` field on the wire (`#[serde(tag =
/// "type")]`). Variant names are `snake_case` on the wire (`page`,
/// `row`, `col`, …).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    // ---- layout ---------------------------------------------------
    /// Root component for a resolved page.
    Page {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        children: Vec<Component>,
    },

    /// Horizontal flex row.
    Row {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<Component>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gap: Option<String>,
    },

    /// Vertical flex column.
    Col {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<Component>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gap: Option<String>,
    },

    /// CSS grid layout.
    Grid {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<Component>,
        /// CSS `grid-template-columns` value, e.g. `"1fr 1fr"`.
        #[serde(skip_serializing_if = "Option::is_none")]
        columns: Option<String>,
    },

    /// Tab container — each tab has a label + child tree.
    Tabs {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        tabs: Vec<Tab>,
    },

    // ---- display --------------------------------------------------
    /// Plain text span.
    Text {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        content: String,
        /// Semantic intent: `"info"`, `"success"`, `"warning"`,
        /// `"danger"`, or `null`.
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },

    /// Section heading (h1–h6).
    Heading {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        content: String,
        /// 1–6, maps to `<h1>`–`<h6>`. Defaults to 2.
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<u8>,
    },

    /// Small status pill / tag.
    Badge {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },

    /// Unified diff display with optional per-line annotations.
    Diff {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        old_text: String,
        new_text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<DiffAnnotation>,
        /// Optional per-line action (e.g. inline comment). `$line`
        /// placeholder is substituted from the click context.
        #[serde(skip_serializing_if = "Option::is_none")]
        line_action: Option<Action>,
    },

    // ---- data -----------------------------------------------------
    /// Server-paginated, sortable table. Rows fetched via
    /// `GET /api/v1/ui/table` (S3); S1 emits the schema only.
    Table {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        source: TableSource,
        columns: Vec<TableColumn>,
        #[serde(skip_serializing_if = "Option::is_none")]
        row_action: Option<Action>,
        #[serde(skip_serializing_if = "Option::is_none")]
        page_size: Option<u32>,
    },

    // ---- input ----------------------------------------------------
    /// Markdown-aware rich-text editor.
    RichText {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },

    // ---- interactive ----------------------------------------------
    /// A button that fires an action on click.
    Button {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        disabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<Action>,
    },

    // ---- composite ------------------------------------------------
    /// JSON-Schema-driven form. `schema_ref` is resolved from
    /// bindings; `submit` fires on form submission.
    Form {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Binding expression or literal schema reference. Resolved
        /// server-side before emission.
        schema_ref: String,
        /// Current form values — resolved from bindings.
        #[serde(skip_serializing_if = "Option::is_none")]
        bindings: Option<JsonValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        submit: Option<Action>,
    },

    // ---- placeholder stubs ----------------------------------------
    /// ACL-redacted widget — the caller lacks permission to see the
    /// bound data. Renderer shows a neutral stub.
    Forbidden { id: String, reason: String },

    /// Widget whose bound node has been deleted. Renderer shows a
    /// neutral "missing" stub.
    Dangling { id: String },

    // ---- escape hatch ---------------------------------------------
    /// Opaque custom component rendered by a plugin-registered
    /// client-side renderer. The server emits `props` verbatim; the
    /// React app looks up `renderer_id` in its component registry and
    /// delegates. Falls back to a neutral stub when the renderer is
    /// not installed.
    ///
    /// Ships in S3 — unblocks UC1 floor-plan, UC2 flow canvas, UC3
    /// state-machine diagram screens before the S4 acceptance demo.
    Custom {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Unique renderer identifier, e.g. `"acme.floorplan"`.
        renderer_id: String,
        /// Opaque props forwarded verbatim to the renderer.
        #[serde(skip_serializing_if = "Option::is_none")]
        props: Option<JsonValue>,
        /// Subscription subjects the renderer wants to watch for live
        /// updates. Mirrors the resolver's subscription plan shape.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        subscribe: Vec<String>,
    },
}

// -------------------------------------------------------------------
// Supporting types
// -------------------------------------------------------------------

/// An action reference carried by interactive components. S2 adds the
/// `/api/v1/ui/action` dispatcher; S1 defines the shape only.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Action {
    /// Handler name registered in the handler registry, e.g.
    /// `"node.update_settings"`.
    pub handler: String,
    /// Opaque arguments forwarded to the handler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<JsonValue>,
}

/// Data source for a [`Component::Table`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableSource {
    /// RSQL query string.
    pub query: String,
    /// Whether the client should subscribe to live updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

/// Column definition for a [`Component::Table`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableColumn {
    pub title: String,
    /// Dot-path into the row object, e.g. `"slots.present_value.value"`.
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sortable: Option<bool>,
}

/// Per-line annotation on a [`Component::Diff`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffAnnotation {
    pub line: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// A single tab inside a [`Component::Tabs`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tab {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub label: String,
    #[serde(default)]
    pub children: Vec<Component>,
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn page_serialises_as_type_page() {
        let c = Component::Page {
            id: "p1".into(),
            title: Some("Hello".into()),
            children: vec![],
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "page");
        assert_eq!(v["id"], "p1");
    }

    #[test]
    fn forbidden_round_trip() {
        let c = Component::Forbidden {
            id: "w1".into(),
            reason: "acl".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Component = serde_json::from_str(&json).unwrap();
        match back {
            Component::Forbidden { id, reason } => {
                assert_eq!(id, "w1");
                assert_eq!(reason, "acl");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn table_with_source_and_columns() {
        let c = Component::Table {
            id: Some("tbl".into()),
            source: TableSource {
                query: "kind==sys.driver.point".into(),
                subscribe: Some(true),
            },
            columns: vec![TableColumn {
                title: "Name".into(),
                field: "path".into(),
                sortable: Some(true),
            }],
            row_action: None,
            page_size: Some(50),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "table");
        assert_eq!(v["source"]["query"], "kind==sys.driver.point");
        assert_eq!(v["columns"][0]["title"], "Name");
    }

    #[test]
    fn form_with_schema_ref() {
        let c = Component::Form {
            id: Some("f1".into()),
            schema_ref: "$target.settings_schema".into(),
            bindings: Some(json!({"name": "test"})),
            submit: Some(Action {
                handler: "node.update_settings".into(),
                args: Some(json!({"target": "$target.id"})),
            }),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "form");
        assert_eq!(v["schema_ref"], "$target.settings_schema");
    }

    #[test]
    fn diff_with_annotations() {
        let c = Component::Diff {
            id: None,
            old_text: "a\nb\n".into(),
            new_text: "a\nc\n".into(),
            language: Some("rust".into()),
            annotations: vec![DiffAnnotation {
                line: 2,
                text: "changed line".into(),
                author: Some("alice".into()),
                created_at: None,
            }],
            line_action: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Component = serde_json::from_str(&json).unwrap();
        match back {
            Component::Diff { annotations, .. } => assert_eq!(annotations.len(), 1),
            other => panic!("expected Diff, got {other:?}"),
        }
    }

    #[test]
    fn component_json_schema() {
        let schema = schemars::schema_for!(Component);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("\"type\""));
    }

    #[test]
    fn custom_escape_hatch_round_trip() {
        let c = Component::Custom {
            id: Some("map1".into()),
            renderer_id: "acme.floorplan".into(),
            props: Some(serde_json::json!({ "floor": 2 })),
            subscribe: vec!["node.123.slot.state".into()],
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "custom");
        assert_eq!(v["renderer_id"], "acme.floorplan");
        assert_eq!(v["subscribe"][0], "node.123.slot.state");
        let back: Component = serde_json::from_value(v).unwrap();
        assert!(matches!(back, Component::Custom { .. }));
    }
}
