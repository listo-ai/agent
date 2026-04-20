//! `sys.compute.math.add` — two-input adder.
//!
//! Stateful across messages because the two inputs don't arrive in the
//! same frame: input `a` fires on port `a`, `b` on port `b`, and the
//! node must remember the last seen value of each to emit `sum = a + b`.
//!
//! Memory lives in status slots (`last_a`, `last_b`), same pattern as
//! `Count`'s `count` slot. Reading the slot fresh every message means a
//! direct `graph.write_slot("/flow/add", "last_a", …)` call — from the
//! CLI, a test, or a flow fixture — is immediately observable in the
//! next emission. No per-instance caching.
//!
//! `sum` emits only when both `last_a` and `last_b` are non-null. The
//! `sum` output slot IS the latest value — no mirror status slot
//! (Stage 7 of NODE-RED-MODEL.md deleted `last_sum`).

use blocks_sdk::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

#[derive(NodeKind)]
#[node(
    kind = "sys.compute.math.add",
    manifest = "manifests/math_add.yaml",
    behavior = "custom"
)]
pub struct Add;

/// No runtime config for the adder today. `serde(default)` on an empty
/// struct keeps the `settings`-slot machinery happy.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddConfig {}

const INPUT_A: &str = "a";
const INPUT_B: &str = "b";
const OUTPUT_SUM: &str = "sum";
const STATUS_A: &str = "last_a";
const STATUS_B: &str = "last_b";

impl NodeBehavior for Add {
    type Config = AddConfig;

    fn on_init(&self, ctx: &NodeCtx, _cfg: &AddConfig) -> Result<(), NodeError> {
        // Best-effort seed — leave slots null so `last_*` faithfully
        // represents "haven't seen a value yet". Don't zero them:
        // `0 + 0 = 0` would look like a real emission to subscribers.
        if ctx.read_status(STATUS_A).is_err() {
            ctx.update_status(STATUS_A, JsonValue::Null)?;
        }
        if ctx.read_status(STATUS_B).is_err() {
            ctx.update_status(STATUS_B, JsonValue::Null)?;
        }
        Ok(())
    }

    fn on_message(&self, ctx: &NodeCtx, port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let n = extract_number(&msg.payload).ok_or_else(|| {
            NodeError::runtime(format!(
                "math.add port `{port}` payload must be numeric, got {:?}",
                msg.payload
            ))
        })?;

        match port.as_ref() {
            INPUT_A => ctx.update_status(STATUS_A, json!(n))?,
            INPUT_B => ctx.update_status(STATUS_B, json!(n))?,
            other => {
                return Err(NodeError::runtime(format!(
                    "math.add received message on unknown port `{other}`"
                )))
            }
        }

        // Always read both fresh — per the slot-source rule an external
        // writer could have set `last_a` or `last_b` behind our back.
        let a = ctx.read_status(STATUS_A)?;
        let b = ctx.read_status(STATUS_B)?;
        let (Some(av), Some(bv)) = (as_number(&a), as_number(&b)) else {
            // Waiting for the other input. No emission.
            return Ok(());
        };
        let sum = av + bv;
        ctx.emit(OUTPUT_SUM, msg.child(json!(sum)))?;
        Ok(())
    }
}

fn extract_number(v: &JsonValue) -> Option<f64> {
    v.as_f64()
}

fn as_number(v: &JsonValue) -> Option<f64> {
    v.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_number_handles_int_and_float() {
        assert_eq!(extract_number(&json!(42)), Some(42.0));
        assert_eq!(extract_number(&json!(1.5)), Some(1.5));
        assert_eq!(extract_number(&json!(null)), None);
        assert_eq!(extract_number(&json!("nope")), None);
    }

    #[test]
    fn sum_waits_for_both_inputs() {
        // Pure logic check — as_number guards emission.
        let a = json!(3);
        let b = json!(null);
        assert!(as_number(&b).is_none());
        assert_eq!(as_number(&a), Some(3.0));
    }
}
