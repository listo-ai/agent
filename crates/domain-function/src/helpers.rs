//! Rhai helpers bound into the per-call Engine.
//!
//! Split from `function.rs` so the wiring code (AST cache, eval loop,
//! port emission) stays focused. Each helper is registered as a native
//! Rust function the script can call; the cost is negligible (Rhai
//! dispatches these faster than script-defined fns).
//!
//! **Design rule for this module:** helpers must be pure of tokio and
//! of any &NodeCtx borrow. Anything that needs those values takes them
//! by `Arc` / clone so the closures stay `Send + 'static` (Rhai's
//! `sync` feature forces `Send+Sync` on registered fns, and we want
//! that invariant too — scripts may outlive the on_message stack frame
//! if we ever move eval off-thread).

use std::str::FromStr;
use std::sync::Arc;

use blocks_sdk::ctx::GraphAccess;
use rhai::{Dynamic, Engine};
use spi::NodePath;

use crate::convert::{json_to_rhai, rhai_to_json};

/// Install every helper the Function node exposes. Called once per
/// eval (cheap — Engine::new + register_fn are µs-scale).
///
/// `graph` is captured by the closures so cross-node slot reads route
/// to the live graph store. `node_path` is captured for `log` /
/// `warn` / `error` so tracing events carry the emitting node's path.
pub(crate) fn install(
    engine: &mut Engine,
    graph: Arc<dyn GraphAccess>,
    node_path: NodePath,
) {
    install_logging(engine, node_path.clone());
    install_time(engine);
    install_json(engine);
    install_msg_helpers(engine);
    install_graph_read(engine, graph, node_path);
}

// ----- logging -----

fn install_logging(engine: &mut Engine, node_path: NodePath) {
    let p = node_path.as_str().to_string();
    let p_log = p.clone();
    engine.register_fn("log", move |s: &str| {
        tracing::info!(target: "domain_function::script", node = %p_log, "{s}");
    });
    let p_warn = p.clone();
    engine.register_fn("warn", move |s: &str| {
        tracing::warn!(target: "domain_function::script", node = %p_warn, "{s}");
    });
    let p_err = p;
    engine.register_fn("error", move |s: &str| {
        tracing::error!(target: "domain_function::script", node = %p_err, "{s}");
    });
}

// ----- time / ids -----

fn install_time(engine: &mut Engine) {
    engine.register_fn("now_ms", || -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    });
    engine.register_fn("uuid", || -> String {
        uuid_v4_like()
    });
}

/// Lightweight UUIDv4-shape string using Rust stdlib RNG — we don't
/// pull `uuid` crate just for this. Collision risk across a flow is
/// negligible; this isn't for crypto.
fn uuid_v4_like() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h1 = RandomState::new().build_hasher();
    let mut h2 = RandomState::new().build_hasher();
    h1.write_i64(now_ns());
    h2.write_i64(now_ns() ^ 0xA5A5_A5A5_A5A5_A5A5u64 as i64);
    let a = h1.finish();
    let b = h2.finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        ((a >> 16) & 0xffff) as u16,
        (a & 0xfff) as u16,
        ((b >> 48) & 0xffff) as u16,
        b & 0xffff_ffff_ffff,
    )
}

fn now_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

// ----- JSON -----

fn install_json(engine: &mut Engine) {
    engine.register_fn("parse_json", |s: &str| -> Dynamic {
        serde_json::from_str::<serde_json::Value>(s)
            .map(json_to_rhai)
            .unwrap_or(Dynamic::UNIT)
    });
    engine.register_fn("to_json", |v: Dynamic| -> String {
        serde_json::to_string(&rhai_to_json(v)).unwrap_or_default()
    });
}

// ----- Msg convenience -----

fn install_msg_helpers(engine: &mut Engine) {
    // `new_msg(payload)` — fresh Msg, no topic
    engine.register_fn("new_msg", |payload: Dynamic| -> Dynamic {
        let mut map = rhai::Map::new();
        map.insert("payload".into(), payload);
        map.insert("_msgid".into(), Dynamic::from(uuid_v4_like()));
        Dynamic::from(map)
    });
    // `new_msg(payload, topic)` — fresh Msg with topic
    engine.register_fn(
        "new_msg",
        |payload: Dynamic, topic: &str| -> Dynamic {
            let mut map = rhai::Map::new();
            map.insert("payload".into(), payload);
            map.insert("topic".into(), Dynamic::from(topic.to_string()));
            map.insert("_msgid".into(), Dynamic::from(uuid_v4_like()));
            Dynamic::from(map)
        },
    );
    // `clone_msg(msg)` — deep clone via JSON round-trip
    engine.register_fn("clone_msg", |m: Dynamic| -> Dynamic {
        json_to_rhai(rhai_to_json(m))
    });
}

// ----- read-only cross-node slot access -----

fn install_graph_read(engine: &mut Engine, graph: Arc<dyn GraphAccess>, _self_path: NodePath) {
    // `get_slot(path, slot)` → Dynamic. Any node's slot, read-only.
    // Returns `()` on miss (unknown path, unknown slot, or slot never
    // written). Scripts distinguish "no value" from "null value" by
    // the Rhai `is_unit` test.
    //
    // Security: no ACL check here. This surface is host-side trusted
    // code; the Studio already gates who can edit the script via
    // Rule B slot writes on `settings`. Operators who can edit
    // settings can already do anything. If we grow cross-user
    // sandboxing, this is the first place a permission check lands.
    let graph_for_read = graph.clone();
    engine.register_fn(
        "get_slot",
        move |path: &str, slot: &str| -> Dynamic {
            match NodePath::from_str(path) {
                Ok(p) => graph_for_read
                    .read_slot(&p, slot)
                    .map(json_to_rhai)
                    .unwrap_or(Dynamic::UNIT),
                Err(_) => Dynamic::UNIT,
            }
        },
    );
}
