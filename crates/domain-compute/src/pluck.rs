//! `sys.compute.pluck` — dot-path projection between nodes.
//!
//! The Node-RED "change / pick" idiom as a first-class kind. Drop one
//! between a source emitting a `Msg` envelope and a downstream node
//! that wants a sub-value. Settings:
//!
//! * `path` — dot-path into the incoming msg. `payload.count` walks
//!   into the payload object. Empty string passes the whole msg value
//!   through (topic-only override).
//! * `topic` — optional override applied to the emitted child.
//!
//! Why a node instead of a wire-level projection: see
//! [`docs/design/NODE-RED-MODEL.md`] §"Why projections are nodes, not
//! wires" — keeps Rule A intact, keeps the transform observable, keeps
//! Node-RED parity.

use blocks_sdk::prelude::*;
use serde::Deserialize;
use serde_json::Value as JsonValue;

#[derive(NodeKind)]
#[node(
    kind = "sys.compute.pluck",
    manifest = "manifests/pluck.yaml",
    behavior = "custom"
)]
pub struct Pluck;

#[derive(Debug, Clone, Deserialize)]
pub struct PluckConfig {
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default)]
    pub topic: Option<String>,
}

fn default_path() -> String {
    "payload".to_string()
}

impl NodeBehavior for Pluck {
    type Config = PluckConfig;

    fn on_init(&self, _ctx: &NodeCtx, _cfg: &PluckConfig) -> Result<(), NodeError> {
        Ok(())
    }

    fn on_message(&self, ctx: &NodeCtx, _port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let cfg = ctx.resolve_settings::<PluckConfig>(&msg)?;
        let msg_json = serde_json::to_value(&msg)
            .map_err(|e| NodeError::runtime(format!("msg not serialisable: {e}")))?;
        let Some(picked) = walk_path(&msg_json, &cfg.path) else {
            // Missing path → drop silently. A warn is appropriate for
            // debugging but not an error; downstream nodes that need a
            // value should validate in their own on_message.
            return Ok(());
        };
        let mut child = msg.child(picked);
        if let Some(t) = cfg.topic.clone() {
            child = child.with_topic(t);
        }
        ctx.emit("out", child)
    }
}

/// Walk a dot-separated path into a JSON value. Returns `None` if any
/// segment is missing or the cursor is a non-object when a key is
/// expected. Empty path returns the root.
fn walk_path(root: &JsonValue, path: &str) -> Option<JsonValue> {
    if path.is_empty() {
        return Some(root.clone());
    }
    let mut cursor = root;
    for segment in path.split('.') {
        cursor = cursor.as_object()?.get(segment)?;
    }
    Some(cursor.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn walk_hits_nested_value() {
        let v = json!({"payload": {"count": 7, "state": true}});
        assert_eq!(walk_path(&v, "payload.count"), Some(json!(7)));
        assert_eq!(walk_path(&v, "payload.state"), Some(json!(true)));
        assert_eq!(
            walk_path(&v, "payload"),
            Some(json!({"count": 7, "state": true}))
        );
    }

    #[test]
    fn walk_empty_returns_root() {
        let v = json!({"payload": 1});
        assert_eq!(walk_path(&v, ""), Some(v.clone()));
    }

    #[test]
    fn walk_missing_returns_none() {
        let v = json!({"payload": {"count": 7}});
        assert_eq!(walk_path(&v, "payload.missing"), None);
        assert_eq!(walk_path(&v, "missing.count"), None);
    }

    #[test]
    fn config_defaults_to_payload_path() {
        let cfg: PluckConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(cfg.path, "payload");
        assert!(cfg.topic.is_none());
    }
}
