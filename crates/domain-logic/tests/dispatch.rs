#![allow(clippy::unwrap_used, clippy::panic)]
//! End-to-end dispatch tests for `acme.logic.trigger`.
//!
//! Timer-dependent paths run under `#[tokio::test(start_paused = true)]`
//! so they advance virtual time deterministically — no wall-clock waits.
//!
//! The full engine worker loop drives dispatch in these tests, which
//! exercises the same code paths the agent will run in production:
//! Scheduler → mpsc → worker_loop → BehaviorRegistry::dispatch_timer.

use std::sync::Arc;
use std::time::Duration;

use engine::{queue, Engine};
use extensions_sdk::NodeKind;
use graph::{GraphStore, KindRegistry};
use serde_json::{json, Value as JsonValue};
use spi::{Msg, NodePath};
use tokio::time;

fn slot(graph: &GraphStore, path: &NodePath, name: &str) -> JsonValue {
    graph
        .get(path)
        .unwrap()
        .slot_values
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, sv)| sv.value)
        .unwrap_or(JsonValue::Null)
}

async fn fire_in(graph: &GraphStore, path: &NodePath) {
    let v = serde_json::to_value(Msg::new(json!({}))).unwrap();
    graph.write_slot(path, "in", v).unwrap();
    yield_dispatch().await;
}

async fn fire_reset(graph: &GraphStore, path: &NodePath) {
    let v = serde_json::to_value(Msg::new(json!({}))).unwrap();
    graph.write_slot(path, "reset", v).unwrap();
    yield_dispatch().await;
}

/// Yield once so the worker loop drains the event channel.
async fn yield_dispatch() {
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn once_emits_then_arms_then_fires_after_delay() {
    let (engine, graph, path) = setup_with(json!({ "mode": "once", "delay_ms": 1000 })).await;

    fire_in(&graph, &path).await;
    assert_eq!(slot(&graph, &path, "armed"), json!(true));
    assert_eq!(
        slot(&graph, &path, "out").get("payload"),
        Some(&json!(true))
    );

    // A second input within the delay window must be ignored.
    let before = slot(&graph, &path, "out");
    fire_in(&graph, &path).await;
    assert_eq!(slot(&graph, &path, "out"), before, "second input ignored");

    time::advance(Duration::from_millis(1001)).await;
    yield_dispatch().await;
    assert_eq!(slot(&graph, &path, "armed"), json!(false), "delay disarms");

    engine.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn extend_debounces_subsequent_inputs() {
    let (engine, graph, path) = setup_with(json!({ "mode": "extend", "delay_ms": 500 })).await;

    fire_in(&graph, &path).await;
    let first = slot(&graph, &path, "out");
    assert_eq!(first.get("payload"), Some(&json!(true)));

    // Halfway through, a fresh input restarts the timer but does not
    // re-emit the trigger payload.
    time::advance(Duration::from_millis(300)).await;
    fire_in(&graph, &path).await;
    assert_eq!(slot(&graph, &path, "out"), first, "no re-emit");

    // Original delay would have elapsed by now (300+300=600 > 500),
    // but the restart pushed it out — node still armed.
    time::advance(Duration::from_millis(300)).await;
    yield_dispatch().await;
    assert_eq!(slot(&graph, &path, "armed"), json!(true));

    // Push past the *new* delay's expiry.
    time::advance(Duration::from_millis(300)).await;
    yield_dispatch().await;
    assert_eq!(slot(&graph, &path, "armed"), json!(false));

    engine.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn manual_reset_only_disarms_via_reset_port() {
    let (engine, graph, path) =
        setup_with(json!({ "mode": "manual_reset", "delay_ms": 0, "reset_payload": "bye" })).await;

    fire_in(&graph, &path).await;
    assert_eq!(slot(&graph, &path, "armed"), json!(true));

    // Time elapses — manual_reset has no timer, so armed stays true.
    time::advance(Duration::from_secs(60)).await;
    yield_dispatch().await;
    assert_eq!(slot(&graph, &path, "armed"), json!(true));

    fire_reset(&graph, &path).await;
    assert_eq!(slot(&graph, &path, "armed"), json!(false));
    assert_eq!(
        slot(&graph, &path, "out").get("payload"),
        Some(&json!("bye"))
    );

    engine.shutdown().await.unwrap();
}

/// SLOT-SOURCE REGRESSION for `armed` — same shape as count's.
/// If the behaviour cached `armed` on a struct field, an out-of-band
/// write to `false` would not let a fresh input emit again.
#[tokio::test(start_paused = true)]
async fn armed_slot_source_regression() {
    let (engine, graph, path) = setup_with(json!({ "mode": "once", "delay_ms": 5_000 })).await;

    fire_in(&graph, &path).await;
    assert_eq!(slot(&graph, &path, "armed"), json!(true));
    let first = slot(&graph, &path, "out");

    // Out-of-band: forcibly disarm via the slot, bypassing the
    // behaviour. A struct-field cache would still consider the node
    // armed and ignore the next input.
    graph.write_slot(&path, "armed", json!(false)).unwrap();

    fire_in(&graph, &path).await;
    let second = slot(&graph, &path, "out");
    assert_ne!(
        second.get("_msgid"),
        first.get("_msgid"),
        "second input must produce a fresh emission once slot says disarmed",
    );
    assert_eq!(second.get("payload"), Some(&json!(true)));

    engine.shutdown().await.unwrap();
}

async fn setup_with(config: JsonValue) -> (Arc<Engine>, Arc<GraphStore>, NodePath) {
    let kinds = KindRegistry::new();
    domain_logic::register_kinds(&kinds);
    let (sink, rx) = queue::channel();
    let graph = Arc::new(GraphStore::new(kinds, sink));

    let kind = <domain_logic::Trigger as NodeKind>::kind_id();
    graph.create_root(kind.clone()).unwrap();
    let path = NodePath::root();
    let id = graph.get(&path).unwrap().id;

    let engine = Engine::new(graph.clone(), rx);
    engine
        .behaviors()
        .register(kind, domain_logic::behavior())
        .unwrap();
    engine.behaviors().set_config(id, config);
    engine.start().await.unwrap();
    engine.behaviors().dispatch_init(id).unwrap();
    yield_dispatch().await;
    (engine, graph, path)
}
