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
    /// Time-series chart. Data points are fetched server-side at
    /// resolve time against the node+slot addressed by `source`. Zoom
    /// / pan gestures write `{from, to}` into `$page[page_state_key]`
    /// and re-issue `/ui/resolve`, so the server can return denser
    /// data for the focused window. Live ticks come in via the
    /// subscription plan — the subject is `node.<id>.slot.<slot>`.
    Chart {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        source: ChartSource,
        /// Series emitted by the server (one per line / area).
        #[serde(default)]
        series: Vec<ChartSeries>,
        /// Current visible window (inclusive ms since epoch). The
        /// server fills this from `$page.<page_state_key>` or its own
        /// default when the client hasn't zoomed yet.
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<ChartRange>,
        /// Client writes zoom / pan state here on `$page`. Defaults to
        /// `"chart_range"` when absent.
        #[serde(skip_serializing_if = "Option::is_none")]
        page_state_key: Option<String>,
        /// `"line"` | `"area"` | `"bar"`. Defaults to `"line"`.
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },

    /// Compact sparkline — a single line of recent points, no axes, no
    /// interaction. Intended for KPI tiles.
    Sparkline {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Inline values, newest last. Server fills these at resolve.
        #[serde(default)]
        values: Vec<f64>,
        /// Subscription subject for live append (`node.<id>.slot.<s>`).
        #[serde(skip_serializing_if = "Option::is_none")]
        subscribe: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },

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

    /// Hierarchical tree — the sidebar/file-browser shape. Children
    /// arrive pre-expanded; lazy-expand is an S7 refinement.
    Tree {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        nodes: Vec<TreeItem>,
        /// Optional click action — `$node.id` is substituted at
        /// dispatch time.
        #[serde(skip_serializing_if = "Option::is_none")]
        node_action: Option<Action>,
    },

    /// Chronological event list. When `subscribe` is set and `mode` is
    /// `"append"`, incoming NATS messages on the subject are appended
    /// to `events` client-side without a tree re-resolve — the
    /// streaming-text story from SDUI.md § "Streaming content".
    Timeline {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        events: Vec<TimelineEvent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscribe: Option<String>,
        /// `"append"` (default — new messages are added to the list)
        /// or `"replace"` (each message replaces the list).
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },

    /// Markdown block. With `subscribe` set, new messages on the
    /// subject append (or replace) the content depending on `mode` —
    /// the UC2 AI-streaming-output primitive.
    Markdown {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscribe: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
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

    /// Node-graph reference picker — user searches/filters nodes in
    /// the graph, picks one; the form stores its id. UC1 alarm rules,
    /// UC2 settings forms, UC3 scope target pickers.
    RefPicker {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// RSQL filter restricting which nodes the picker offers.
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        /// Current value — a node id.
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },

    /// Multi-step form. Each step has a nested child tree rendered
    /// one at a time; `submit` fires when the last step is confirmed.
    Wizard {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        steps: Vec<WizardStep>,
        #[serde(skip_serializing_if = "Option::is_none")]
        submit: Option<Action>,
    },

    /// Off-canvas slide-over panel. `open` is bound from `$page`; the
    /// close gesture writes `false` back.
    Drawer {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        open: bool,
        /// `$page` key that owns the open state. Defaults to
        /// `drawer_<id>`.
        #[serde(skip_serializing_if = "Option::is_none")]
        page_state_key: Option<String>,
        #[serde(default)]
        children: Vec<Component>,
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

/// An action reference carried by interactive components.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Action {
    /// Handler name registered in the handler registry, e.g.
    /// `"node.update_settings"`.
    pub handler: String,
    /// Opaque arguments forwarded to the handler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<JsonValue>,
    /// Client-side hint for making the UI feel instant: apply this
    /// patch to the tree immediately on click; when the server
    /// responds, either it confirms (no-op) or it returns an
    /// authoritative Patch/FullRender that replaces the optimistic
    /// one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimistic: Option<OptimisticHint>,
}

/// Client-side optimistic-update hint — see SDUI.md § "Optimistic
/// hints". Applied before the round-trip fires; the server's response
/// overrides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OptimisticHint {
    /// Target IR component id to patch. The client walks the current
    /// tree, finds the node with this id, and shallow-merges `fields`
    /// into it.
    pub target_component_id: String,
    /// Object of field-name → value pairs to merge into the target
    /// component. `serde_json::Value` so any typed field can be
    /// updated.
    pub fields: JsonValue,
}

/// Data source for a [`Component::Chart`] — a node + slot that carries
/// a time series.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChartSource {
    /// Node id (UUID as string) — the subscription plan is derived
    /// from this plus `slot`.
    pub node_id: String,
    /// Slot name on the node (e.g. `"value"`, `"current_count"`).
    pub slot: String,
}

/// One series in a [`Component::Chart`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChartSeries {
    /// Display label.
    pub label: String,
    /// Point list — `[ts_ms, value]` pairs, oldest first.
    #[serde(default)]
    pub points: Vec<(i64, f64)>,
}

/// Inclusive time window (ms since epoch) for a chart.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct ChartRange {
    pub from: i64,
    pub to: i64,
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

/// One node inside a [`Component::Tree`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TreeItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub children: Vec<TreeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// One event inside a [`Component::Timeline`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimelineEvent {
    /// RFC 3339 timestamp or raw ms-since-epoch string.
    pub ts: String,
    pub text: String,
    /// `"info"` | `"ok"` | `"warn"` | `"danger"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

/// One step of a [`Component::Wizard`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WizardStep {
    pub label: String,
    #[serde(default)]
    pub children: Vec<Component>,
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
                optimistic: None,
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
