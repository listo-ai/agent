//! One-click presets for the manual-test UI.
//!
//! Not a public API — these exist so you can click a button and get
//! back a known-good graph shape (a count chain, a trigger demo) rather
//! than rebuilding it every restart. Production graphs come from flow
//! documents in Stage 2b, extension installs in Stage 10, or the UI.

use std::str::FromStr;

use graph::{GraphStore, SlotRef};
use serde::{Deserialize, Serialize};
use serde_json::json;
use spi::{KindId, NodePath};

use crate::routes::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Folder → count (sys.compute.count) → trigger (sys.logic.trigger),
    /// `count.out` wired into `trigger.in`.
    CountChain,
    /// Folder → trigger (sys.logic.trigger) with default config. Write
    /// anything to `in` and watch `armed` flip.
    TriggerDemo,
    /// Dashboard demo: ui.nav → ui.page → ui.widget hierarchy for
    /// exercising `agent ui nav` and `agent ui resolve` end-to-end.
    UiDemo,
}

#[derive(Debug, Serialize)]
pub struct SeededNode {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct SeedResult {
    pub folder: String,
    pub nodes: Vec<SeededNode>,
    pub links: Vec<String>,
}

/// Run a preset. Fails fast if the folder name is already taken — we
/// never silently clobber. Also seeds default configs + fires `on_init`
/// so the chain is immediately live-wire-testable: one click then one
/// slot write per the NODE-SCOPE "can you see the flow" acceptance.
pub(crate) fn apply(state: &AppState, preset: Preset) -> Result<SeedResult, ApiError> {
    if matches!(preset, Preset::UiDemo) {
        return apply_ui_demo(state);
    }

    let (folder_name, kinds) = match preset {
        Preset::CountChain => (
            "count_chain",
            vec![
                ("count", "sys.compute.count"),
                ("trigger", "sys.logic.trigger"),
            ],
        ),
        Preset::TriggerDemo => ("trigger_demo", vec![("trigger", "sys.logic.trigger")]),
        Preset::UiDemo => unreachable!(),
    };

    let graph: &GraphStore = &state.graph;
    let folder_path =
        NodePath::from_str(&format!("/{folder_name}")).expect("literal path is valid");
    let root = NodePath::root();
    graph
        .create_child(&root, KindId::new("sys.core.folder"), folder_name)
        .map_err(ApiError::from_graph)?;

    let mut nodes = Vec::new();
    let mut node_ids = Vec::new();
    for (name, kind) in &kinds {
        let id = graph
            .create_child(&folder_path, KindId::new(*kind), name)
            .map_err(ApiError::from_graph)?;
        let cfg = default_config(kind);
        state
            .behaviors
            .set_config(id, cfg)
            .map_err(|e| ApiError::bad_request(format!("set settings failed for `{name}`: {e}")))?;
        state
            .behaviors
            .dispatch_init(id)
            .map_err(|e| ApiError::bad_request(format!("on_init failed for `{name}`: {e}")))?;
        nodes.push(SeededNode {
            path: folder_path.child(name).to_string(),
            kind: (*kind).to_string(),
        });
        node_ids.push(id);
    }

    let mut links = Vec::new();
    if matches!(preset, Preset::CountChain) {
        // count.out → trigger.in
        let src = SlotRef::new(node_ids[0], "out");
        let tgt = SlotRef::new(node_ids[1], "in");
        let link_id = graph.add_link(src, tgt).map_err(ApiError::from_graph)?;
        links.push(link_id.to_string());
    }

    Ok(SeedResult {
        folder: folder_path.to_string(),
        nodes,
        links,
    })
}

/// Seed a dashboard demo hierarchy for exercising `agent ui nav` and
/// `agent ui resolve`:
///
/// ```text
/// /ui_demo/                 sys.core.folder
/// /ui_demo/home             ui.nav        (title="Home", path="home", frame_alias="current")
/// /ui_demo/overview         ui.page       (title="Overview", standalone layout)
/// /ui_demo/overview/kpi     ui.widget     (widget_type="sys.widgets.kpi", binding value→$stack[-1].id)
/// ```
///
/// Dashboard nodes are passive (no behaviour), so we skip `set_config` /
/// `dispatch_init` and write config slots directly via `graph.write_slot`.
fn apply_ui_demo(state: &AppState) -> Result<SeedResult, ApiError> {
    let graph: &GraphStore = &state.graph;
    let root = NodePath::root();

    // ── folder ────────────────────────────────────────────────────────────
    let folder_path = NodePath::from_str("/ui_demo").expect("literal");
    graph
        .create_child(&root, KindId::new("sys.core.folder"), "ui_demo")
        .map_err(ApiError::from_graph)?;

    let mut nodes = Vec::new();

    // ── ui.nav: home ──────────────────────────────────────────────────────
    graph
        .create_child(&folder_path, KindId::new("ui.nav"), "home")
        .map_err(ApiError::from_graph)?;
    let nav_path = folder_path.child("home");
    graph
        .write_slot(&nav_path, "title", json!("Home"))
        .map_err(|e| ApiError::bad_request(format!("nav title: {e}")))?;
    graph
        .write_slot(&nav_path, "path", json!("home"))
        .map_err(|e| ApiError::bad_request(format!("nav path: {e}")))?;
    graph
        .write_slot(&nav_path, "frame_alias", json!("current"))
        .map_err(|e| ApiError::bad_request(format!("nav frame_alias: {e}")))?;
    nodes.push(SeededNode {
        path: nav_path.to_string(),
        kind: "ui.nav".to_string(),
    });

    // ── ui.page: overview (child of folder, not nav — nav's may_contain
    //    allows only ui.nav children) ─────────────────────────────────────
    let page_id = graph
        .create_child(&folder_path, KindId::new("ui.page"), "overview")
        .map_err(ApiError::from_graph)?;
    let page_path = folder_path.child("overview");
    graph
        .write_slot(&page_path, "title", json!("Overview"))
        .map_err(|e| ApiError::bad_request(format!("page title: {e}")))?;
    graph
        .write_slot(&page_path, "layout", json!({ "type": "grid", "cols": 3 }))
        .map_err(|e| ApiError::bad_request(format!("page layout: {e}")))?;
    // Back-fill the nav's frame_ref now that we know the page id.
    graph
        .write_slot(&nav_path, "frame_ref", json!({ "id": page_id.to_string() }))
        .map_err(|e| ApiError::bad_request(format!("nav frame_ref: {e}")))?;
    nodes.push(SeededNode {
        path: page_path.to_string(),
        kind: "ui.page".to_string(),
    });

    // ── ui.widget: kpi (child of page) ────────────────────────────────────
    graph
        .create_child(&page_path, KindId::new("ui.widget"), "kpi")
        .map_err(ApiError::from_graph)?;
    let widget_path = page_path.child("kpi");
    graph
        .write_slot(&widget_path, "widget_type", json!("sys.widgets.kpi"))
        .map_err(|e| ApiError::bad_request(format!("widget_type: {e}")))?;
    graph
        .write_slot(
            &widget_path,
            "bindings",
            json!({ "value": "$stack[-1].id" }),
        )
        .map_err(|e| ApiError::bad_request(format!("bindings: {e}")))?;
    nodes.push(SeededNode {
        path: widget_path.to_string(),
        kind: "ui.widget".to_string(),
    });

    Ok(SeedResult {
        folder: folder_path.to_string(),
        nodes,
        links: vec![],
    })
}

fn default_config(kind: &str) -> serde_json::Value {
    match kind {
        "sys.compute.count" => json!({
            "initial": 0, "step": 1, "min": null, "max": null, "wrap": false,
        }),
        "sys.logic.trigger" => json!({
            "mode": "once",
            "trigger_payload": true,
            "reset_payload": null,
            "delay_ms": 0,
        }),
        _ => serde_json::Value::Null,
    }
}
