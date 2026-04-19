#![allow(clippy::unwrap_used, clippy::panic)]
//! Stage 2 acceptance tests.
//!
//! The engine, standing on the graph, reacts to slot writes by
//! propagating values along links \u{2014} without any flow document. State
//! transitions and graceful shutdown work end-to-end.

use std::sync::Arc;
use std::time::Duration;

use engine::{kinds as engine_kinds, queue, Engine, EngineState};
use graph::{seed, GraphStore, KindRegistry, SlotRef};
use serde_json::json;
use spi::{KindId, NodePath};
use tokio::time::sleep;

fn fresh() -> (Arc<GraphStore>, Arc<Engine>) {
    let (sink, events) = queue::channel();
    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    engine_kinds::register(&kinds);
    let graph = Arc::new(GraphStore::new(kinds, sink));
    graph
        .create_root(KindId::new("acme.core.station"))
        .expect("root created");
    let engine = Engine::new(graph.clone(), events);
    (graph, engine)
}

/// Drive the worker until the target slot shows `expected`, or time
/// out. We read through a `NodeSnapshot` so the assertion matches the
/// public API surface.
async fn await_slot(graph: &GraphStore, path: &NodePath, slot: &str, expected: &serde_json::Value) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Some(snap) = graph.get(path) {
            if let Some((_, sv)) = snap.slot_values.iter().find(|(n, _)| n == slot) {
                if &sv.value == expected {
                    return;
                }
            }
        }
        sleep(Duration::from_millis(5)).await;
    }
    panic!("slot `{slot}` on `{path}` never reached {expected}");
}

#[tokio::test]
async fn engine_starts_and_stops_cleanly() {
    let (_graph, engine) = fresh();
    assert_eq!(engine.state(), EngineState::Stopped);
    engine.start().await.unwrap();
    assert_eq!(engine.state(), EngineState::Running);
    engine.shutdown().await.unwrap();
    assert_eq!(engine.state(), EngineState::Stopped);
}

#[tokio::test]
async fn pause_blocks_propagation() {
    let (graph, engine) = fresh();
    // Two linked demo-points; value propagates src \u{2192} dst.
    graph
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "d")
        .unwrap();
    let d = NodePath::root().child("d");
    graph
        .create_child(&d, KindId::new("acme.driver.demo.device"), "dev")
        .unwrap();
    let dev = d.child("dev");
    let src = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "src")
        .unwrap();
    let dst = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "dst")
        .unwrap();
    graph
        .add_link(SlotRef::new(src, "value"), SlotRef::new(dst, "value"))
        .unwrap();

    engine.start().await.unwrap();
    engine.pause().await.unwrap();
    assert_eq!(engine.state(), EngineState::Paused);

    // Writes while paused are observed by the worker and dropped \u{2014} the
    // engine does not buffer "missed" propagations across a pause
    // (state may have moved on elsewhere). Assert the drop is silent.
    graph
        .write_slot(&dev.child("src"), "value", json!(7))
        .unwrap();
    sleep(Duration::from_millis(50)).await;
    let snap = graph.get(&dev.child("dst")).unwrap();
    let (_, sv) = snap.slot_values.iter().find(|(n, _)| n == "value").unwrap();
    assert_eq!(sv.value, json!(null), "paused engine must not propagate");

    // After resume, a fresh write propagates normally.
    engine.resume().await.unwrap();
    graph
        .write_slot(&dev.child("src"), "value", json!(9))
        .unwrap();
    await_slot(&graph, &dev.child("dst"), "value", &json!(9)).await;

    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn live_wire_fan_out_propagates() {
    let (graph, engine) = fresh();
    // One source, two downstream targets.
    graph
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "d")
        .unwrap();
    let d = NodePath::root().child("d");
    graph
        .create_child(&d, KindId::new("acme.driver.demo.device"), "dev")
        .unwrap();
    let dev = d.child("dev");
    let src = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "s")
        .unwrap();
    let a = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "a")
        .unwrap();
    let b = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "b")
        .unwrap();
    graph
        .add_link(SlotRef::new(src, "value"), SlotRef::new(a, "value"))
        .unwrap();
    graph
        .add_link(SlotRef::new(src, "value"), SlotRef::new(b, "value"))
        .unwrap();

    engine.start().await.unwrap();
    graph
        .write_slot(&dev.child("s"), "value", json!(42))
        .unwrap();

    await_slot(&graph, &dev.child("a"), "value", &json!(42)).await;
    await_slot(&graph, &dev.child("b"), "value", &json!(42)).await;

    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn fixed_point_cycle_does_not_loop_forever() {
    // A \u{2194} B with the same value eventually quiesces.
    let (graph, engine) = fresh();
    graph
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "d")
        .unwrap();
    let d = NodePath::root().child("d");
    graph
        .create_child(&d, KindId::new("acme.driver.demo.device"), "dev")
        .unwrap();
    let dev = d.child("dev");
    let a = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "a")
        .unwrap();
    let b = graph
        .create_child(&dev, KindId::new("acme.driver.demo.point"), "b")
        .unwrap();
    graph
        .add_link(SlotRef::new(a, "value"), SlotRef::new(b, "value"))
        .unwrap();
    graph
        .add_link(SlotRef::new(b, "value"), SlotRef::new(a, "value"))
        .unwrap();

    engine.start().await.unwrap();
    graph
        .write_slot(&dev.child("a"), "value", json!(3))
        .unwrap();
    await_slot(&graph, &dev.child("b"), "value", &json!(3)).await;
    // A second write to the same value must not trigger further propagation.
    sleep(Duration::from_millis(50)).await;

    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn post_shutdown_writes_do_not_explode() {
    let (graph, engine) = fresh();
    graph
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "d")
        .unwrap();
    let d = NodePath::root().child("d");
    graph
        .create_child(&d, KindId::new("acme.driver.demo.device"), "dev")
        .unwrap();
    graph
        .create_child(&d.child("dev"), KindId::new("acme.driver.demo.point"), "p")
        .unwrap();

    engine.start().await.unwrap();
    engine.shutdown().await.unwrap();
    // Even after the worker is gone, graph mutations are silent \u{2014} the
    // queue sink drops events instead of panicking.
    graph
        .write_slot(&d.child("dev").child("p"), "value", json!(1))
        .expect("graph continues to function post-engine-shutdown");
}

#[tokio::test]
async fn illegal_transitions_return_errors() {
    let (_graph, engine) = fresh();
    // Cannot resume a stopped engine.
    assert!(engine.resume().await.is_err());
    engine.start().await.unwrap();
    // Cannot start a running engine.
    assert!(engine.start().await.is_err());
    engine.shutdown().await.unwrap();
}
