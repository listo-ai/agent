//! Msg ↔ Rhai conversion.
//!
//! The Function node's hot path. Every inbound msg becomes a
//! `rhai::Map`; the script's return becomes a `Msg`. Node-RED parity
//! means the shape authors see is flat — `msg.payload`, `msg.topic`,
//! `msg._msgid`, plus any flattened metadata keys (`msg.qos`,
//! `msg.retain`, …) — not a nested `msg.metadata.qos`.
//!
//! Design choices:
//!
//! * **JSON is the pivot format.** `spi::Msg` serialises to a flat JSON
//!   object (thanks to `#[serde(flatten)]` on `metadata`); Rhai has a
//!   lossless JSON-ish `Dynamic`. Going via JSON is one extra allocation
//!   but keeps the code a small set of obvious match arms and keeps the
//!   semantics identical to what travels on the wire.
//!
//! * **Numbers: i64 preferred, f64 if fractional.** Rhai distinguishes
//!   INT (i64) and FLOAT (f64); serde_json distinguishes `u64` / `i64` /
//!   `f64`. Unsigned integers larger than `i64::MAX` are promoted to
//!   f64 to avoid silent truncation — pathological but documented.
//!
//! * **Null is `()` (Rhai unit).** Rhai has no null; scripts use `()`
//!   to mean "no value" and that's what our conversion emits.
//!
//! * **Round-trip is NOT byte-identical** for every input. In
//!   particular, a script that does `msg.payload = #{}` round-trips to
//!   an empty JSON object, not the original `null`. Tests pin the
//!   common paths; exotic shapes are documented rather than defended.

use rhai::{Array, Dynamic, Map};
use serde_json::Value as JsonValue;
use spi::Msg;

use crate::error::FunctionError;

/// Build the `msg` Rhai Map that the script sees. Flat shape:
/// `{ payload, topic, _msgid, …metadata }`.
pub fn msg_to_rhai(msg: &Msg) -> Result<Dynamic, FunctionError> {
    let json = serde_json::to_value(msg).map_err(|e| FunctionError::MsgSerialise(e.to_string()))?;
    Ok(json_to_rhai(json))
}

/// Fold a Rhai return value back into a `Msg`.
///
/// Accepted shapes:
///
/// * A Map with at least a `payload` key — treated as a full Msg
///   (topic + metadata harvested from the remaining keys).
/// * Any other scalar/collection — promoted to `Msg::new(value)` with
///   a fresh id, preserving Node-RED's "return a bare payload" idiom.
pub fn rhai_to_msg(value: Dynamic) -> Result<Msg, FunctionError> {
    let json = rhai_to_json(value);
    // If the script returned a map shaped like a Msg (has `payload`),
    // deserialize directly — this preserves topic, _msgid, and any
    // user-added fields. Otherwise treat the whole value as the payload.
    if let JsonValue::Object(ref m) = json {
        if m.contains_key("payload") {
            return serde_json::from_value::<Msg>(json)
                .map_err(|e| FunctionError::MsgDeserialise(e.to_string()));
        }
    }
    Ok(Msg::new(json))
}

/// Collect a Rhai return into a list of emittable messages.
///
/// * `()` → empty (drop)
/// * an Array of maps → one Msg per entry
/// * anything else → single-element list via [`rhai_to_msg`]
pub fn rhai_to_msg_batch(value: Dynamic) -> Result<Vec<Msg>, FunctionError> {
    if value.is_unit() {
        return Ok(Vec::new());
    }
    if let Some(arr) = value.clone().try_cast::<Array>() {
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            // Skip unit entries so `[msg, (), msg]` means "emit first
            // and third on `out`" — matches Node-RED's `return [msg1,
            // null, msg2]` treating null as drop.
            if item.is_unit() {
                continue;
            }
            out.push(rhai_to_msg(item)?);
        }
        return Ok(out);
    }
    Ok(vec![rhai_to_msg(value)?])
}

pub fn json_to_rhai(value: JsonValue) -> Dynamic {
    match value {
        JsonValue::Null => Dynamic::UNIT,
        JsonValue::Bool(b) => Dynamic::from(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from_int(i)
            } else if let Some(u) = n.as_u64() {
                // i64 overflow — promote to f64 (documented above).
                i64::try_from(u)
                    .map(Dynamic::from_int)
                    .unwrap_or_else(|_| Dynamic::from_float(u as f64))
            } else {
                Dynamic::from_float(n.as_f64().unwrap_or(0.0))
            }
        }
        JsonValue::String(s) => Dynamic::from(s),
        JsonValue::Array(items) => {
            let arr: Array = items.into_iter().map(json_to_rhai).collect();
            Dynamic::from(arr)
        }
        JsonValue::Object(obj) => {
            let map: Map = obj
                .into_iter()
                .map(|(k, v)| (k.into(), json_to_rhai(v)))
                .collect();
            Dynamic::from(map)
        }
    }
}

pub fn rhai_to_json(value: Dynamic) -> JsonValue {
    if value.is_unit() {
        return JsonValue::Null;
    }
    if let Ok(b) = value.as_bool() {
        return JsonValue::Bool(b);
    }
    if let Ok(i) = value.as_int() {
        return JsonValue::Number(i.into());
    }
    if let Ok(f) = value.as_float() {
        return serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null);
    }
    if value.is_string() {
        return JsonValue::String(value.into_string().unwrap_or_default());
    }
    if let Some(arr) = value.clone().try_cast::<Array>() {
        return JsonValue::Array(arr.into_iter().map(rhai_to_json).collect());
    }
    if let Some(map) = value.clone().try_cast::<Map>() {
        let mut obj = serde_json::Map::with_capacity(map.len());
        for (k, v) in map {
            obj.insert(k.to_string(), rhai_to_json(v));
        }
        return JsonValue::Object(obj);
    }
    // Any other Rhai type (Blob, timestamps, custom) — serialise via
    // its Debug repr as a fallback. Not pretty but preserves data.
    JsonValue::String(format!("{value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn msg_to_rhai_exposes_flat_shape() {
        let msg = Msg::new(json!({"temp": 72}))
            .with_topic("sensors")
            .with_meta("qos", json!(1));
        let d = msg_to_rhai(&msg).unwrap();
        let map = d.try_cast::<Map>().unwrap();
        assert_eq!(
            rhai_to_json(map.get("topic").unwrap().clone()),
            json!("sensors")
        );
        assert_eq!(rhai_to_json(map.get("qos").unwrap().clone()), json!(1));
        assert_eq!(
            rhai_to_json(map.get("payload").unwrap().clone()),
            json!({"temp": 72})
        );
    }

    #[test]
    fn rhai_to_msg_preserves_topic_and_meta() {
        let mut map = Map::new();
        map.insert("payload".into(), Dynamic::from_int(42));
        map.insert("topic".into(), Dynamic::from("t"));
        map.insert("qos".into(), Dynamic::from_int(1));
        let msg = rhai_to_msg(Dynamic::from(map)).unwrap();
        assert_eq!(msg.payload, json!(42));
        assert_eq!(msg.topic.as_deref(), Some("t"));
        assert_eq!(msg.metadata.get("qos"), Some(&json!(1)));
    }

    #[test]
    fn rhai_bare_value_becomes_payload() {
        let msg = rhai_to_msg(Dynamic::from_int(7)).unwrap();
        assert_eq!(msg.payload, json!(7));
        assert!(msg.topic.is_none());
    }

    #[test]
    fn rhai_unit_batch_is_empty() {
        assert!(rhai_to_msg_batch(Dynamic::UNIT).unwrap().is_empty());
    }

    #[test]
    fn rhai_array_batch_emits_each_and_skips_unit() {
        let mut a = Map::new();
        a.insert("payload".into(), Dynamic::from_int(1));
        let mut b = Map::new();
        b.insert("payload".into(), Dynamic::from_int(2));
        let arr: Array = vec![Dynamic::from(a), Dynamic::UNIT, Dynamic::from(b)];
        let out = rhai_to_msg_batch(Dynamic::from(arr)).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].payload, json!(1));
        assert_eq!(out[1].payload, json!(2));
    }

    #[test]
    fn json_round_trip_preserves_nested_shape() {
        let v = json!({"a": [1, "two", {"nested": true}], "b": null});
        let back = rhai_to_json(json_to_rhai(v.clone()));
        assert_eq!(back, v);
    }
}
