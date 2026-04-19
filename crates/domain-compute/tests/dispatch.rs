#![allow(clippy::unwrap_used, clippy::panic)]
//! End-to-end dispatch tests for `acme.compute.count`.
//!
//! Each test wires a graph store + behaviour registry + SDK behaviour
//! and exercises the dispatch path that ships in Stage 3a-2 — without
//! standing up the full async engine. The registry is the unit under
//! test for slot-source compliance and resolution-order correctness.

use std::sync::Arc;

use engine::BehaviorRegistry;
use extensions_sdk::NodeKind;
use graph::{GraphEvent, GraphStore, KindRegistry, NullSink, VecSink};
use serde_json::{json, Value as JsonValue};
use spi::{KindId, Msg, NodePath};

fn make() -> (Arc<GraphStore>, Arc<VecSink>, BehaviorRegistry, NodePath) {
    let kinds = KindRegistry::new();
    domain_compute::register_kinds(&kinds);
    let sink = Arc::new(VecSink::new());
    let graph = Arc::new(GraphStore::new(kinds, sink.clone()));

    // Free leaf — root must accept it. Use Count as its own root for
    // test isolation; the manifest declares no `must_live_under`.
    let kind = <domain_compute::Count as NodeKind>::kind_id();
    graph
        .create_root(kind.clone())
        .expect("create root for count");
    let path = NodePath::root();

    let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
    behaviors
        .register(kind, domain_compute::behavior())
        .expect("register count behaviour");

    (graph, sink, behaviors, path)
}

fn fire(behaviors: &BehaviorRegistry, sink: &VecSink) {
    for ev in sink.take() {
        behaviors.handle(&ev);
    }
}

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

#[test]
fn on_init_seeds_count_from_initial() {
    let (graph, sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 7, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();
    let _ = sink.take();
    assert_eq!(slot(&graph, &path, "count"), json!(7));
}

#[test]
fn on_message_increments_and_emits() {
    let (graph, sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 0, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();
    sink.take();

    let trigger = serde_json::to_value(Msg::new(json!({}))).unwrap();
    graph.write_slot(&path, "in", trigger).unwrap();
    fire(&behaviors, &sink);

    assert_eq!(slot(&graph, &path, "count"), json!(1));
    let out = slot(&graph, &path, "out");
    assert_eq!(out.get("payload"), Some(&json!(1)));
}

/// SLOT-SOURCE REGRESSION (per `docs/sessions/STEPS.md` Stage 3a-2).
///
/// If the behaviour cached "current count" in a struct field, an
/// out-of-band write to the slot would be invisible — the next
/// increment would derive from the cached value, not the slot. By
/// reading `count` fresh in `on_message`, an external write to 42
/// must produce an emitted output of 43, not 11.
#[test]
fn slot_source_regression_external_write_wins() {
    let (graph, sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 10, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();
    sink.take();

    // Out-of-band slot write — bypasses the behaviour entirely.
    graph.write_slot(&path, "count", json!(42)).unwrap();
    sink.take();

    // Trigger an increment.
    let trigger = serde_json::to_value(Msg::new(json!({}))).unwrap();
    graph.write_slot(&path, "in", trigger).unwrap();
    fire(&behaviors, &sink);

    let out = slot(&graph, &path, "out");
    assert_eq!(
        out.get("payload"),
        Some(&json!(43)),
        "behaviour must read live slot, not cached state"
    );
    assert_eq!(slot(&graph, &path, "count"), json!(43));
}

#[test]
fn reset_via_port_returns_to_initial() {
    let (graph, sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 100, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();
    sink.take();

    graph.write_slot(&path, "count", json!(999)).unwrap();
    sink.take();

    let m = serde_json::to_value(Msg::new(json!({}))).unwrap();
    graph.write_slot(&path, "reset", m).unwrap();
    fire(&behaviors, &sink);

    assert_eq!(slot(&graph, &path, "count"), json!(100));
}

#[test]
fn reset_via_msg_metadata_returns_to_initial() {
    let (graph, sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 50, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();
    sink.take();

    graph.write_slot(&path, "count", json!(777)).unwrap();
    sink.take();

    let mut m = Msg::new(json!({}));
    m.metadata.insert("reset".into(), json!(true));
    graph
        .write_slot(&path, "in", serde_json::to_value(&m).unwrap())
        .unwrap();
    fire(&behaviors, &sink);

    assert_eq!(slot(&graph, &path, "count"), json!(50));
}

#[test]
fn msg_step_override_beats_config() {
    let (graph, sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 0, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();
    sink.take();

    let mut m = Msg::new(json!({}));
    m.metadata.insert("step".into(), json!(5));
    graph
        .write_slot(&path, "in", serde_json::to_value(&m).unwrap())
        .unwrap();
    fire(&behaviors, &sink);

    assert_eq!(slot(&graph, &path, "count"), json!(5));
}

/// Status / config writes must NOT re-enter the behaviour. Without this
/// guard `update_status` would recurse forever.
#[test]
fn status_writes_do_not_dispatch() {
    let (graph, _sink, behaviors, path) = make();
    let id = graph.get(&path).unwrap().id;
    behaviors.set_config(id, json!({ "initial": 0, "step": 1 }));
    behaviors.dispatch_init(id).unwrap();

    // Synthesise a SlotChanged for the status slot — the dispatcher
    // must ignore it.
    let event = GraphEvent::SlotChanged {
        id,
        path: path.clone(),
        slot: "count".into(),
        value: json!(123),
        generation: 99,
    };
    behaviors.handle(&event); // must not panic, must not increment

    // Sanity: the slot is still whatever the synthetic event carried —
    // the dispatcher didn't react.
    assert_eq!(slot(&graph, &path, "count"), json!(0));
}

/// Fan-out smoke: a NullSink graph (no dispatch) still behaves for the
/// purely-graph caller. This pins that registering the kind is safe
/// even when the engine isn't running.
#[test]
fn kind_registers_without_dispatch() {
    let kinds = KindRegistry::new();
    domain_compute::register_kinds(&kinds);
    let graph = GraphStore::new(kinds, Arc::new(NullSink));
    let kind = <domain_compute::Count as NodeKind>::kind_id();
    graph.create_root(kind).expect("root creation");
    assert_eq!(graph.len(), 1);
}

#[test]
fn requires_declares_spi_msg() {
    let r = domain_compute::requires();
    assert!(r
        .iter()
        .any(|req| req.id == spi::capabilities::platform::spi_msg()));
}

#[test]
fn count_kind_id_matches_manifest() {
    assert_eq!(
        <domain_compute::Count as NodeKind>::kind_id(),
        KindId::new("acme.compute.count")
    );
}
