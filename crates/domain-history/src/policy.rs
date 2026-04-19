//! Per-slot recording policy and COV change-detection logic.
//!
//! COV semantics per type (SLOT-STORAGE.md §"COV semantics per type"):
//! - Number  — `|new - last_recorded| > deadband`
//! - Bool    — new != last_recorded (byte equality)
//! - String  — byte equality
//! - Json    — structural deep-equal up to 64 KiB; byte equality beyond
//! - Binary  — byte equality, subject to size cap
//!
//! All types respect `min_interval_ms` (rate floor) and `max_gap_ms`
//! (heartbeat ceiling).

use serde_json::Value as JsonValue;
use spi::SlotValueKind;

/// Maximum JSON document size for structural equality check.
/// Beyond this we fall back to cheap byte comparison (serialized form).
const STRUCTURAL_EQ_SIZE_LIMIT: usize = 64 * 1024;

/// Resolved kind of policy for one slot.
#[derive(Debug, Clone)]
pub enum PolicyKind {
    Cov {
        deadband: f64,
        min_interval_ms: u64,
        max_gap_ms: u64,
    },
    Interval {
        period_ms: u64,
        align_to_wall: bool,
    },
    OnDemand,
}

/// A resolved, ready-to-apply policy for one (parent_node, slot_name) pair.
#[derive(Debug, Clone)]
pub struct EffectivePolicy {
    pub kind: PolicyKind,
    /// Effective sample cap for this slot (slot override → config → platform).
    pub max_samples: u64,
}

/// Decides whether a new JSON value should be recorded given the last
/// recorded value, the slot's declared kind, and the active COV config.
///
/// Returns `true` if the record should be written (change detected or
/// heartbeat overdue), `false` if the rate floor / no-change rules
/// suppress the write.
///
/// The `elapsed_ms` parameter is the monotonic time since the last record
/// was written for this slot (or `u64::MAX` on the first sample).
pub fn should_record_cov(
    new_value: &JsonValue,
    last_value: Option<&JsonValue>,
    value_kind: SlotValueKind,
    deadband: f64,
    min_interval_ms: u64,
    max_gap_ms: u64,
    elapsed_ms: u64,
) -> bool {
    // Heartbeat ceiling: always record if we haven't written in max_gap_ms.
    if elapsed_ms >= max_gap_ms {
        return true;
    }
    // Rate floor: never record faster than min_interval_ms.
    if min_interval_ms > 0 && elapsed_ms < min_interval_ms {
        return false;
    }
    // The last value is unknown on the very first sample — always record.
    let last = match last_value {
        Some(v) => v,
        None => return true,
    };
    // Change detection per type.
    match value_kind {
        SlotValueKind::Number => {
            let new = coerce_f64(new_value);
            let old = coerce_f64(last);
            match (new, old) {
                (Some(n), Some(o)) => (n - o).abs() > deadband,
                // If either fails to parse as a number, treat as changed.
                _ => true,
            }
        }
        SlotValueKind::Bool => new_value != last,
        SlotValueKind::String => {
            // String slots: compare as raw JSON strings (already UTF-8).
            new_value != last
        }
        SlotValueKind::Json => json_changed(new_value, last),
        SlotValueKind::Binary => {
            // Binary values arrive as base64-encoded JSON strings.
            new_value != last
        }
        SlotValueKind::Null => false, // Null always stable.
    }
}

fn coerce_f64(v: &JsonValue) -> Option<f64> {
    v.as_f64()
}

/// Structural equality up to `STRUCTURAL_EQ_SIZE_LIMIT`; byte equality beyond.
fn json_changed(new_value: &JsonValue, last: &JsonValue) -> bool {
    // Quick path: identical by value equality (covers numbers, booleans, null).
    if new_value == last {
        return false;
    }
    // For large documents, serialize both and compare bytes to avoid the
    // recursive walk that could spike CPU on 1 MB objects.
    let size_hint = std::mem::size_of_val(new_value);
    if size_hint > STRUCTURAL_EQ_SIZE_LIMIT {
        let new_s = serde_json::to_string(new_value).unwrap_or_default();
        let old_s = serde_json::to_string(last).unwrap_or_default();
        return new_s != old_s;
    }
    // Full structural check already done above via `==` on JsonValue.
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn number_deadband() {
        // Change within deadband → not recorded.
        assert!(!should_record_cov(
            &json!(10.3),
            Some(&json!(10.0)),
            SlotValueKind::Number,
            0.5,
            0,
            900_000,
            1000,
        ));
        // Change exceeds deadband → recorded.
        assert!(should_record_cov(
            &json!(10.6),
            Some(&json!(10.0)),
            SlotValueKind::Number,
            0.5,
            0,
            900_000,
            1000,
        ));
    }

    #[test]
    fn heartbeat_forces_record() {
        // Even with no change, heartbeat ceiling fires.
        assert!(should_record_cov(
            &json!(10.0),
            Some(&json!(10.0)),
            SlotValueKind::Number,
            0.5,
            0,
            900_000,
            900_001, // elapsed > max_gap
        ));
    }

    #[test]
    fn rate_floor_suppresses() {
        assert!(!should_record_cov(
            &json!(true),
            Some(&json!(false)),
            SlotValueKind::Bool,
            0.0,
            5000, // min_interval_ms
            900_000,
            100, // elapsed < min_interval
        ));
    }

    #[test]
    fn first_sample_always_recorded() {
        assert!(should_record_cov(
            &json!(42.0),
            None,
            SlotValueKind::Number,
            0.5,
            0,
            900_000,
            0,
        ));
    }

    #[test]
    fn json_structural_eq() {
        // Same object in different key order is not `==` in serde_json
        // but should still be considered unchanged when values are equivalent.
        let a = json!({"x": 1, "y": 2});
        let b = json!({"y": 2, "x": 1});
        // serde_json Value::Object preserves insertion order but == compares by content
        // so this actually comes out equal — confirm no false positive.
        assert!(!json_changed(&a, &b));
    }
}
