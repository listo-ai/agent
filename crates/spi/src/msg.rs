//! Node-RED-compatible message envelope for wires between slots.
//!
//! See `docs/design/EVERYTHING-AS-NODE.md` § "Wires, ports, and messages".
//!
//! # Wire-level JSON layout
//!
//! ```json
//! {
//!   "payload": <any>,
//!   "topic": "optional",
//!   "_msgid": "uuid",
//!   "_parentid": "uuid or absent",
//!   "_ts": 1700000000000,
//!   "_source": "acme/station/folder/node",
//!   "userField1": ...,
//!   "userField2": ...
//! }
//! ```
//!
//! `payload`, `topic`, `_msgid`, and user-added root-level fields match
//! Node-RED exactly — Node-RED flows can be imported and Function-node
//! JS that reads/writes those fields works unchanged.
//!
//! `_ts`, `_parentid`, `_source` are our additions, underscore-prefixed
//! so they're clearly platform-reserved and don't collide with user
//! fields.
//!
//! # Immutability
//!
//! `Msg` is an immutable value on the wire. Nodes that transform a
//! message produce a new one (typically via [`Msg::child`] for
//! provenance). This kills the Node-RED fan-out mutation bug where two
//! downstream branches mutate a shared `msg` and see each other's
//! changes. The QuickJS Function node exposes `msg` as mutable JS for
//! authoring familiarity; the runtime snapshots it into a fresh `Msg`
//! on exit.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Opaque message identifier. Serialises transparently as a string so
/// Node-RED clients and JSON consumers can treat it as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Node-RED-compatible message envelope.
///
/// Custom user fields land in [`metadata`](Self::metadata); they serialise
/// flattened onto the root JSON object so that Node-RED's
/// `msg.myCustomField = ...` round-trips byte-for-byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    /// Primary data. Same semantics as Node-RED's `msg.payload`.
    pub payload: serde_json::Value,

    /// Optional routing / grouping hint. Same semantics as Node-RED's
    /// `msg.topic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,

    /// Message identifier. Serialises as `_msgid` for Node-RED parity.
    #[serde(rename = "_msgid", default)]
    pub id: MessageId,

    /// The message this one was derived from, if any. Underscore-prefixed
    /// so it's clearly platform-reserved.
    #[serde(rename = "_parentid", default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<MessageId>,

    /// Creation timestamp, milliseconds since Unix epoch.
    #[serde(rename = "_ts", default = "now_millis")]
    pub timestamp_ms: u64,

    /// Emitting node's path in the graph, if known.
    #[serde(rename = "_source", default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// User-added custom fields. Flattened onto the root JSON on
    /// serialize, harvested from unknown root-level keys on deserialize.
    ///
    /// Keys starting with underscore are reserved for platform use;
    /// putting one here on the Rust side won't collide with a
    /// recognised field but *may* be overridden when merged with
    /// future platform-reserved keys — avoid it.
    #[serde(flatten, default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl Msg {
    /// Build a new message with a fresh id and current timestamp.
    pub fn new(payload: serde_json::Value) -> Self {
        Self {
            payload,
            topic: None,
            id: MessageId::new(),
            parent_id: None,
            timestamp_ms: now_millis(),
            source: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Derive a child message carrying this one as its `parent_id`.
    /// Copies `topic` forward (matching Node-RED intuition); does not
    /// copy `source` (the emitting node will set its own) or
    /// `metadata` (the caller opts in if they want it carried).
    pub fn child(&self, payload: serde_json::Value) -> Self {
        Self {
            payload,
            topic: self.topic.clone(),
            id: MessageId::new(),
            parent_id: Some(self.id),
            timestamp_ms: now_millis(),
            source: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Chainable setters for ergonomics. These consume and return self.
    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialises_node_red_shape() {
        let msg = Msg::new(json!({"temp": 72}))
            .with_topic("sensors/floor3")
            .with_source("acme/station/devices/floor3/temp-1")
            .with_meta("unit", json!("F"));

        let v: serde_json::Value = serde_json::to_value(&msg).unwrap();

        // Node-RED-compatible keys
        assert_eq!(v["payload"], json!({"temp": 72}));
        assert_eq!(v["topic"], json!("sensors/floor3"));
        assert!(
            v["_msgid"].is_string(),
            "id serialises as _msgid for Node-RED parity"
        );

        // Platform-reserved keys
        assert!(v["_ts"].is_number());
        assert_eq!(v["_source"], json!("acme/station/devices/floor3/temp-1"));

        // Custom field is flattened onto the root, Node-RED style
        assert_eq!(v["unit"], json!("F"));
    }

    #[test]
    fn deserialises_node_red_input_with_custom_fields() {
        // A message a Node-RED flow might emit: custom fields at root level,
        // _msgid from Node-RED's core.
        let raw = json!({
            "payload": 42,
            "topic": "t",
            "_msgid": "00000000-0000-0000-0000-000000000000",
            "customA": "a",
            "customB": [1, 2, 3],
        });

        let msg: Msg = serde_json::from_value(raw).unwrap();

        assert_eq!(msg.payload, json!(42));
        assert_eq!(msg.topic.as_deref(), Some("t"));
        assert_eq!(msg.id.0, Uuid::nil());
        assert_eq!(msg.metadata.get("customA"), Some(&json!("a")));
        assert_eq!(msg.metadata.get("customB"), Some(&json!([1, 2, 3])));
    }

    #[test]
    fn round_trip_preserves_custom_fields() {
        let msg = Msg::new(json!("hi"))
            .with_meta("a", json!(1))
            .with_meta("b", json!({"nested": true}));

        let bytes = serde_json::to_vec(&msg).unwrap();
        let back: Msg = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(back.payload, json!("hi"));
        assert_eq!(back.metadata.get("a"), Some(&json!(1)));
        assert_eq!(back.metadata.get("b"), Some(&json!({"nested": true})));
        assert_eq!(back.id, msg.id);
    }

    #[test]
    fn child_tracks_parent() {
        let parent = Msg::new(json!(1)).with_topic("t");
        let child = parent.child(json!(2));

        assert_eq!(child.parent_id, Some(parent.id));
        assert_ne!(child.id, parent.id);
        assert_eq!(child.topic.as_deref(), Some("t"), "topic carries forward");
        assert_eq!(child.payload, json!(2));
    }

    #[test]
    fn missing_optional_fields_deserialise_to_defaults() {
        // Minimal Node-RED-ish payload — no _msgid, no topic, no custom.
        let raw = json!({ "payload": "bare" });
        let msg: Msg = serde_json::from_value(raw).unwrap();

        assert_eq!(msg.payload, json!("bare"));
        assert!(msg.topic.is_none());
        assert!(msg.parent_id.is_none());
        assert!(msg.source.is_none());
        assert!(msg.metadata.is_empty());
        // id defaulted to a fresh UUID
        assert_ne!(msg.id.0, Uuid::nil());
    }
}
