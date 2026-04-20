//! Node-RED-compatible message envelope for wires between slots.
//!
//! See `docs/design/EVERYTHING-AS-NODE.md` § "Wires, ports, and messages"
//! and `docs/design/NODE-RED-MODEL.md` § "Msg shape change".
//!
//! # Wire-level JSON layout
//!
//! ```json
//! {
//!   "payload": <any>,
//!   "topic": "optional",
//!   "_msgid": "uuid",
//!   "userField1": ...,
//!   "userField2": ...
//! }
//! ```
//!
//! Three fields. `payload`, `topic`, `_msgid`, plus arbitrary user-added
//! root-level fields — Node-RED byte-for-byte. Stripped in Stage 2:
//! `_ts` (SSE frame + history writer stamp their own timestamps),
//! `_source` (derived from the subject / span attributes), `_parentid`
//! (carried as W3C TraceContext in transport metadata once Stage 1.5
//! wiring lands; none of the three had production readers).
//!
//! # Immutability
//!
//! `Msg` is an immutable value on the wire. Nodes that transform a
//! message produce a new one via [`Msg::child`] or [`Msg::new`]. This
//! kills the Node-RED fan-out mutation bug where two downstream
//! branches mutate a shared `msg` and see each other's changes. The
//! QuickJS Function node exposes `msg` as mutable JS for authoring
//! familiarity; the runtime snapshots it into a fresh `Msg` on exit.

use std::collections::BTreeMap;

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
    /// Build a new message with a fresh id.
    pub fn new(payload: serde_json::Value) -> Self {
        Self {
            payload,
            topic: None,
            id: MessageId::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Derive a child message. Copies `topic` forward (matching
    /// Node-RED intuition); does not copy `metadata` (the caller opts
    /// in if they want it carried). Parent lineage now travels as
    /// W3C TraceContext in transport metadata, not on the msg itself.
    pub fn child(&self, payload: serde_json::Value) -> Self {
        Self {
            payload,
            topic: self.topic.clone(),
            id: MessageId::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialises_node_red_shape() {
        let msg = Msg::new(json!({"temp": 72}))
            .with_topic("sensors/floor3")
            .with_meta("unit", json!("F"));

        let v: serde_json::Value = serde_json::to_value(&msg).unwrap();

        assert_eq!(v["payload"], json!({"temp": 72}));
        assert_eq!(v["topic"], json!("sensors/floor3"));
        assert!(
            v["_msgid"].is_string(),
            "id serialises as _msgid for Node-RED parity"
        );
        assert!(v.get("_ts").is_none(), "_ts stripped in Stage 2");
        assert!(v.get("_source").is_none(), "_source stripped in Stage 2");
        assert!(
            v.get("_parentid").is_none(),
            "_parentid stripped in Stage 2"
        );

        // Custom field is flattened onto the root, Node-RED style
        assert_eq!(v["unit"], json!("F"));
    }

    #[test]
    fn deserialises_node_red_input_with_custom_fields() {
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
    fn deserialises_legacy_msg_ignoring_stripped_fields() {
        // Stage-2 forward-compat: a producer that still emits _ts /
        // _source / _parentid (e.g., an old fixture, a pre-upgrade
        // edge agent) must still parse — the fields flow into
        // `metadata` as opaque underscore-keyed values and are
        // ignored everywhere downstream.
        let raw = json!({
            "payload": 1,
            "_msgid": "00000000-0000-0000-0000-000000000001",
            "_ts": 1700000000000u64,
            "_source": "acme/node",
            "_parentid": "00000000-0000-0000-0000-000000000002",
        });
        let msg: Msg = serde_json::from_value(raw).unwrap();
        assert_eq!(msg.payload, json!(1));
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
    fn child_has_fresh_id_and_carries_topic() {
        let parent = Msg::new(json!(1)).with_topic("t");
        let child = parent.child(json!(2));

        assert_ne!(child.id, parent.id);
        assert_eq!(child.topic.as_deref(), Some("t"), "topic carries forward");
        assert_eq!(child.payload, json!(2));
    }

    #[test]
    fn missing_optional_fields_deserialise_to_defaults() {
        let raw = json!({ "payload": "bare" });
        let msg: Msg = serde_json::from_value(raw).unwrap();

        assert_eq!(msg.payload, json!("bare"));
        assert!(msg.topic.is_none());
        assert!(msg.metadata.is_empty());
        // id defaulted to a fresh UUID
        assert_ne!(msg.id.0, Uuid::nil());
    }
}
