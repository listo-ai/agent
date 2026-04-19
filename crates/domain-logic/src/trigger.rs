//! `sys.logic.trigger` — Node-RED-style trigger / debounce / arm node.
//!
//! Three modes:
//!   * `once`         — first input emits trigger payload, arms the
//!     node; subsequent inputs ignored until the delay elapses or
//!     reset arrives.
//!   * `extend`       — debounce: each new input restarts the delay
//!     without re-emitting the trigger payload.
//!   * `manual_reset` — emit trigger payload on every input; only a
//!     reset (port or `msg.reset`) clears the armed state.
//!
//! State (`armed`, `pending_timer`) lives in **status slots**, not on
//! the struct. The slot-source regression test in `tests/dispatch.rs`
//! pins this rule for the `armed` slot.

use extensions_sdk::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

#[derive(NodeKind)]
#[node(
    kind = "sys.logic.trigger",
    manifest = "manifests/trigger.yaml",
    behavior = "custom"
)]
pub struct Trigger;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    Once,
    Extend,
    ManualReset,
}

impl Default for TriggerMode {
    fn default() -> Self {
        Self::Once
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriggerConfig {
    #[serde(default)]
    pub mode: TriggerMode,
    #[serde(default = "default_trigger_payload")]
    pub trigger_payload: JsonValue,
    #[serde(default)]
    pub reset_payload: JsonValue,
    #[serde(default)]
    pub delay_ms: u64,
}

fn default_trigger_payload() -> JsonValue {
    json!(true)
}

impl NodeBehavior for Trigger {
    type Config = TriggerConfig;

    fn on_init(&self, ctx: &NodeCtx, _cfg: &TriggerConfig) -> Result<(), NodeError> {
        ctx.update_status("armed", json!(false))?;
        ctx.update_status("pending_timer", JsonValue::Null)
    }

    fn on_message(&self, ctx: &NodeCtx, port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let cfg = ctx.resolve_settings::<TriggerConfig>(&msg)?;
        let resetting = port == "reset" || msg.metadata.get("reset") == Some(&json!(true));

        if resetting {
            return reset(ctx, &cfg, &msg);
        }
        handle_input(ctx, &cfg, &msg)
    }

    fn on_timer(&self, ctx: &NodeCtx, handle: TimerHandle) -> Result<(), NodeError> {
        // Stale-fire guard: a cancelled timer might still race the
        // dispatcher in a future scheduler. Compare the fired handle
        // against the slot — only the *current* pending timer is
        // honoured.
        let pending = ctx.read_status("pending_timer")?.as_u64();
        if pending != Some(handle.0) {
            return Ok(());
        }
        let cfg = ctx.resolve_settings::<TriggerConfig>(&Msg::new(JsonValue::Null))?;
        finish(ctx, &cfg, &Msg::new(JsonValue::Null))
    }
}

fn handle_input(ctx: &NodeCtx, cfg: &TriggerConfig, msg: &Msg) -> Result<(), NodeError> {
    let armed = ctx.read_status("armed")?.as_bool().unwrap_or(false);
    match cfg.mode {
        TriggerMode::Once => {
            if armed {
                return Ok(());
            }
            emit_trigger(ctx, cfg, msg)?;
            ctx.update_status("armed", json!(true))?;
            schedule_if_set(ctx, cfg)
        }
        TriggerMode::Extend => {
            if !armed {
                emit_trigger(ctx, cfg, msg)?;
                ctx.update_status("armed", json!(true))?;
            } else {
                cancel_pending(ctx)?;
            }
            schedule_if_set(ctx, cfg)
        }
        TriggerMode::ManualReset => {
            emit_trigger(ctx, cfg, msg)?;
            ctx.update_status("armed", json!(true))
        }
    }
}

fn emit_trigger(ctx: &NodeCtx, cfg: &TriggerConfig, msg: &Msg) -> Result<(), NodeError> {
    ctx.emit("out", msg.child(cfg.trigger_payload.clone()))
}

fn schedule_if_set(ctx: &NodeCtx, cfg: &TriggerConfig) -> Result<(), NodeError> {
    if cfg.delay_ms == 0 {
        return Ok(());
    }
    let h = ctx.schedule(cfg.delay_ms)?;
    ctx.update_status("pending_timer", json!(h.0))
}

fn cancel_pending(ctx: &NodeCtx) -> Result<(), NodeError> {
    if let Some(id) = ctx.read_status("pending_timer")?.as_u64() {
        ctx.cancel(TimerHandle(id));
    }
    ctx.update_status("pending_timer", JsonValue::Null)
}

fn reset(ctx: &NodeCtx, cfg: &TriggerConfig, msg: &Msg) -> Result<(), NodeError> {
    cancel_pending(ctx)?;
    finish(ctx, cfg, msg)
}

fn finish(ctx: &NodeCtx, cfg: &TriggerConfig, msg: &Msg) -> Result<(), NodeError> {
    ctx.update_status("armed", json!(false))?;
    ctx.update_status("pending_timer", JsonValue::Null)?;
    if !cfg.reset_payload.is_null() {
        ctx.emit("out", msg.child(cfg.reset_payload.clone()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_default_is_once() {
        assert_eq!(TriggerMode::default(), TriggerMode::Once);
    }

    #[test]
    fn config_deserialises_with_defaults() {
        let cfg: TriggerConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(cfg.mode, TriggerMode::Once);
        assert_eq!(cfg.trigger_payload, json!(true));
        assert_eq!(cfg.delay_ms, 0);
    }
}
