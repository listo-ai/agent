//! `sys.logic.heartbeat` — self-driven boolean toggle with a counter.
//!
//! Source node: no inputs. On `on_init` it arms a timer and emits the
//! initial `{ state, count }` on the single `out` port. Each timer fire
//! flips the state, increments the counter, and emits one message
//! carrying both fields under `msg.payload`.
//!
//! (docs/design/NODE-RED-MODEL.md) deleted the mirror status
//! slots `current_state` / `current_count`: the output slot IS the
//! current value, so the behaviour recovers its previous state by
//! reading the `out` slot back (the engine persists every emit). Only
//! `pending_timer` remains as a status slot — marked `isInternal` so
//! it doesn't clutter the node card.
//!
//! A stale-fire guard (pattern borrowed from `trigger`) ignores timer
//! fires whose handle no longer matches the active one — defends
//! against the cancel-vs-fire race when the user edits settings while a
//! timer is pending.

use blocks_sdk::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

#[derive(NodeKind)]
#[node(
    kind = "sys.logic.heartbeat",
    manifest = "manifests/heartbeat.yaml",
    behavior = "custom"
)]
pub struct Heartbeat;

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatConfig {
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default)]
    pub start_state: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_interval_ms() -> u64 {
    1000
}
fn default_enabled() -> bool {
    true
}

const OUT: &str = "out";
const PENDING_TIMER: &str = "pending_timer";

impl NodeBehavior for Heartbeat {
    type Config = HeartbeatConfig;

    // No input slots on this kind, so `on_message` is never called by
    // the dispatcher. Required by the trait; unreachable in practice.
    fn on_message(&self, _ctx: &NodeCtx, _port: InputPort, _msg: Msg) -> Result<(), NodeError> {
        Ok(())
    }

    fn on_init(&self, ctx: &NodeCtx, cfg: &HeartbeatConfig) -> Result<(), NodeError> {
        cancel_pending(ctx);
        let (state, count) = prev_state(ctx).unwrap_or((cfg.start_state, 0));
        ctx.emit(OUT, Msg::new(json!({ "state": state, "count": count })))?;
        arm_if_enabled(ctx, cfg)
    }

    fn on_timer(&self, ctx: &NodeCtx, handle: TimerHandle) -> Result<(), NodeError> {
        let pending = ctx.read_status(PENDING_TIMER).ok().and_then(|v| v.as_u64());
        if pending != Some(handle.0) {
            return Ok(());
        }

        let cfg = ctx.resolve_settings::<HeartbeatConfig>(&Msg::new(JsonValue::Null))?;
        if !cfg.enabled {
            ctx.update_status(PENDING_TIMER, JsonValue::Null)?;
            return Ok(());
        }

        let (prev_state, prev_count) = prev_state(ctx).unwrap_or((cfg.start_state, 0));
        let next_state = !prev_state;
        let next_count = prev_count + 1;

        ctx.emit(
            OUT,
            Msg::new(json!({ "state": next_state, "count": next_count })),
        )?;

        arm_if_enabled(ctx, &cfg)
    }
}

/// Recover `(state, count)` from the last `Msg` emitted on `out`.
/// Returns `None` on first init (slot is null / missing a shape).
fn prev_state(ctx: &NodeCtx) -> Option<(bool, u64)> {
    let out = ctx.read_output(OUT).ok()?;
    let payload = out.get("payload")?;
    let state = payload.get("state")?.as_bool()?;
    let count = payload.get("count")?.as_u64()?;
    Some((state, count))
}

fn arm_if_enabled(ctx: &NodeCtx, cfg: &HeartbeatConfig) -> Result<(), NodeError> {
    if !cfg.enabled {
        return ctx.update_status(PENDING_TIMER, JsonValue::Null);
    }
    let handle = ctx.schedule(cfg.interval_ms)?;
    ctx.update_status(PENDING_TIMER, json!(handle.0))
}

fn cancel_pending(ctx: &NodeCtx) {
    if let Some(id) = ctx.read_status(PENDING_TIMER).ok().and_then(|v| v.as_u64()) {
        ctx.cancel(TimerHandle(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg: HeartbeatConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(cfg.interval_ms, 1000);
        assert!(!cfg.start_state);
        assert!(cfg.enabled);
    }

    #[test]
    fn config_accepts_partial_override() {
        let cfg: HeartbeatConfig = serde_json::from_value(json!({"interval_ms": 500})).unwrap();
        assert_eq!(cfg.interval_ms, 500);
        assert!(cfg.enabled);
    }
}
