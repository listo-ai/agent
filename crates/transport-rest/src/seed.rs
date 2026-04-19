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
    /// Folder → count (acme.compute.count) → trigger (acme.logic.trigger),
    /// `count.out` wired into `trigger.in`.
    CountChain,
    /// Folder → trigger (acme.logic.trigger) with default config. Write
    /// anything to `in` and watch `armed` flip.
    TriggerDemo,
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
    let (folder_name, kinds) = match preset {
        Preset::CountChain => (
            "count_chain",
            vec![
                ("count", "acme.compute.count"),
                ("trigger", "acme.logic.trigger"),
            ],
        ),
        Preset::TriggerDemo => ("trigger_demo", vec![("trigger", "acme.logic.trigger")]),
    };

    let graph: &GraphStore = &state.graph;
    let folder_path =
        NodePath::from_str(&format!("/{folder_name}")).expect("literal path is valid");
    let root = NodePath::root();
    graph
        .create_child(&root, KindId::new("acme.core.folder"), folder_name)
        .map_err(ApiError::from_graph)?;

    let mut nodes = Vec::new();
    let mut node_ids = Vec::new();
    for (name, kind) in &kinds {
        let id = graph
            .create_child(&folder_path, KindId::new(*kind), name)
            .map_err(ApiError::from_graph)?;
        let cfg = default_config(kind);
        state.behaviors.set_config(id, cfg);
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
        links.push(link_id.0.to_string());
    }

    Ok(SeedResult {
        folder: folder_path.to_string(),
        nodes,
        links,
    })
}

fn default_config(kind: &str) -> serde_json::Value {
    match kind {
        "acme.compute.count" => json!({
            "initial": 0, "step": 1, "min": null, "max": null, "wrap": false,
        }),
        "acme.logic.trigger" => json!({
            "mode": "once",
            "trigger_payload": true,
            "reset_payload": null,
            "delay_ms": 0,
        }),
        _ => serde_json::Value::Null,
    }
}
