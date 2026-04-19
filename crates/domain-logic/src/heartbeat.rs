//! `sys.logic.heartbeat` — self-driven boolean toggle with a counter.
//!
//! Source node: no inputs. On `on_init` it emits `start_state` on the
//! `state` output and arms a timer. Each timer fire flips the state,
//! increments the counter, and emits both on the output ports. Status
//! mirrors (`current_state`, `current_count`) hold the readable state
//! because [`NodeCtx::read_status`] only reads status-role slots — the
//! output ports are write-only from the behaviour's POV (same idiom as
//! [`crate::trigger`]).
//!
//! A stale-fire guard (pattern borrowed from `trigger`) ignores timer
//! fires whose handle no longer matches the active one — defends
//! against the cancel-vs-fire race when the user edits settings while a
//! timer is pending.

use extensions_sdk::prelude::*;
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

const STATE_OUT: &str = "state";
const COUNT_OUT: &str = "count";
const CURRENT_STATE: &str = "current_state";
const CURRENT_COUNT: &str = "current_count";
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
        // Preserve the counter across config edits and restarts — only
        // seed it when the slot is still null (first-ever init).
        let count = ctx
            .read_status(CURRENT_COUNT)
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let state = ctx
            .read_status(CURRENT_STATE)
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(cfg.start_state);
        ctx.update_status(CURRENT_STATE, json!(state))?;
        ctx.update_status(CURRENT_COUNT, json!(count))?;
        ctx.emit(STATE_OUT, Msg::new(json!(state)))?;
        ctx.emit(COUNT_OUT, Msg::new(json!(count)))?;
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

        let prev = ctx
            .read_status(CURRENT_STATE)
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(cfg.start_state);
        let next = !prev;
        let prev_count = ctx
            .read_status(CURRENT_COUNT)
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let next_count = prev_count + 1;

        ctx.update_status(CURRENT_STATE, json!(next))?;
        ctx.update_status(CURRENT_COUNT, json!(next_count))?;
        ctx.emit(STATE_OUT, Msg::new(json!(next)))?;
        ctx.emit(COUNT_OUT, Msg::new(json!(next_count)))?;

        arm_if_enabled(ctx, &cfg)
    }
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
