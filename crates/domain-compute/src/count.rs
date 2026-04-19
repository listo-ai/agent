//! `sys.compute.count` — incrementing counter node.
//!
//! Two trigger inputs (`in`, `reset`); one output (`out`); one status
//! slot (`count`). Step / bounds / wrap configurable.
//!
//! **State lives in the `count` slot, not on `Count`.** That's the rule
//! the slot-source regression test pins: writing the slot directly via
//! the graph store is observable in the *next* `on_message`, because
//! the behaviour reads `count` fresh every time. Caching it on a struct
//! field would break this test by design.

use extensions_sdk::prelude::*;
use serde::Deserialize;

#[derive(NodeKind)]
#[node(
    kind = "sys.compute.count",
    manifest = "manifests/count.yaml",
    behavior = "custom"
)]
pub struct Count;

#[derive(Debug, Clone, Deserialize)]
pub struct CountConfig {
    #[serde(default)]
    pub initial: i64,
    #[serde(default = "default_step")]
    pub step: i64,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub wrap: bool,
}

fn default_step() -> i64 {
    1
}

impl NodeBehavior for Count {
    type Config = CountConfig;

    fn on_init(&self, ctx: &NodeCtx, cfg: &CountConfig) -> Result<(), NodeError> {
        ctx.update_status("count", cfg.initial.into())
    }

    fn on_message(&self, ctx: &NodeCtx, port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let cfg = ctx.resolve_settings::<CountConfig>(&msg)?;

        let reset = port == "reset" || msg.metadata.get("reset") == Some(&serde_json::json!(true));

        if reset {
            let value: serde_json::Value = cfg.initial.into();
            ctx.update_status("count", value.clone())?;
            ctx.emit("out", msg.child(value))?;
            return Ok(());
        }

        // Always read the live slot — no per-instance caching. This is
        // what makes the slot-source regression test passable.
        let current = ctx
            .read_status("count")?
            .as_i64()
            .ok_or_else(|| NodeError::runtime("count slot is not an integer"))?;

        let next = apply_step(current, cfg.step, cfg.min, cfg.max, cfg.wrap);
        ctx.update_status("count", next.into())?;
        ctx.emit("out", msg.child(next.into()))?;
        Ok(())
    }
}

/// Pure arithmetic for the count step. Pulled out so it's unit-testable
/// without touching the runtime.
pub fn apply_step(cur: i64, step: i64, min: Option<i64>, max: Option<i64>, wrap: bool) -> i64 {
    let raw = cur.saturating_add(step);
    match (min, max, wrap) {
        (Some(lo), Some(hi), true) if raw > hi => lo + (raw - hi - 1).rem_euclid(hi - lo + 1),
        (Some(lo), Some(hi), true) if raw < lo => hi - (lo - raw - 1).rem_euclid(hi - lo + 1),
        (Some(lo), _, _) if raw < lo => lo,
        (_, Some(hi), _) if raw > hi => hi,
        _ => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_basic() {
        assert_eq!(apply_step(0, 1, None, None, false), 1);
        assert_eq!(apply_step(10, 5, None, None, false), 15);
    }

    #[test]
    fn step_negative() {
        assert_eq!(apply_step(5, -2, None, None, false), 3);
    }

    #[test]
    fn step_clamps_to_bounds() {
        assert_eq!(apply_step(9, 5, Some(0), Some(10), false), 10);
        assert_eq!(apply_step(0, -5, Some(0), Some(10), false), 0);
    }

    #[test]
    fn step_wraps_when_configured() {
        // 10 + 1 with [0..=10] wrap → 0
        assert_eq!(apply_step(10, 1, Some(0), Some(10), true), 0);
        // 0 - 1 with [0..=10] wrap → 10
        assert_eq!(apply_step(0, -1, Some(0), Some(10), true), 10);
    }
}
