#![allow(clippy::unwrap_used, clippy::panic)]
//! End-to-end dispatch tests for `sys.logic.function`.
//!
//! These drive the full engine worker loop: write to `in`, let the
//! dispatcher fire `on_message`, then inspect the output slot. Mirror
//! of `domain-logic/tests/dispatch.rs`.

use std::sync::Arc;

use blocks_sdk::NodeKind;
use engine::{queue, Engine};
use graph::{GraphStore, KindRegistry};
use serde_json::{json, Value as JsonValue};
use spi::{Msg, NodePath};

async fn yield_dispatch() {
    for _ in 0..4 {
        tokio::task::yield_now().await;
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

async fn fire_in(graph: &GraphStore, path: &NodePath, msg: Msg) {
    let v = serde_json::to_value(msg).unwrap();
    graph.write_slot(path, "in", v).unwrap();
    yield_dispatch().await;
}

async fn setup_with(script: &str) -> (Arc<Engine>, Arc<GraphStore>, NodePath) {
    setup_with_config(json!({ "script": script })).await
}

async fn setup_with_config(config: JsonValue) -> (Arc<Engine>, Arc<GraphStore>, NodePath) {
    let kinds = KindRegistry::new();
    domain_function::register_kinds(&kinds);
    let (sink, rx) = queue::channel();
    let graph = Arc::new(GraphStore::new(kinds, sink));

    let kind = <domain_function::Function as NodeKind>::kind_id();
    graph.create_root(kind.clone()).unwrap();
    let path = NodePath::root();
    let id = graph.get(&path).unwrap().id;

    let engine = Engine::new(graph.clone(), rx);
    engine
        .behaviors()
        .register(kind, domain_function::behavior())
        .unwrap();
    engine.behaviors().set_config(id, config).unwrap();
    engine.start().await.unwrap();
    engine.behaviors().dispatch_init(id).unwrap();
    yield_dispatch().await;
    (engine, graph, path)
}

#[tokio::test]
async fn identity_script_passes_msg_through() {
    let (engine, graph, path) = setup_with("msg").await;
    fire_in(&graph, &path, Msg::new(json!({"n": 1}))).await;
    let out = slot(&graph, &path, "out");
    assert_eq!(out.get("payload"), Some(&json!({"n": 1})));
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn mutating_payload_round_trips() {
    let (engine, graph, path) = setup_with("msg.payload = msg.payload * 2; msg").await;
    fire_in(&graph, &path, Msg::new(json!(21))).await;
    assert_eq!(slot(&graph, &path, "out").get("payload"), Some(&json!(42)));
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn topic_override_survives() {
    let (engine, graph, path) = setup_with(r#"msg.topic = "renamed"; msg"#).await;
    fire_in(&graph, &path, Msg::new(json!(1)).with_topic("orig")).await;
    assert_eq!(slot(&graph, &path, "out").get("topic"), Some(&json!("renamed")));
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn returning_unit_drops_the_msg() {
    // Drop when payload > 10.
    let (engine, graph, path) =
        setup_with(r#"if msg.payload > 10 { () } else { msg }"#).await;

    fire_in(&graph, &path, Msg::new(json!(5))).await;
    let first_id = slot(&graph, &path, "out").get("_msgid").cloned();
    assert!(first_id.is_some(), "first msg should have emitted");

    // Same script, bigger value — should be dropped, out slot unchanged.
    fire_in(&graph, &path, Msg::new(json!(99))).await;
    let second_id = slot(&graph, &path, "out").get("_msgid").cloned();
    assert_eq!(first_id, second_id, "second msg must not have emitted");

    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn new_msg_helper_builds_fresh_envelope() {
    let (engine, graph, path) = setup_with(r#"new_msg(#{ doubled: msg.payload * 2 }, "dbl")"#).await;
    fire_in(&graph, &path, Msg::new(json!(7))).await;
    let out = slot(&graph, &path, "out");
    assert_eq!(out.get("payload"), Some(&json!({"doubled": 14})));
    assert_eq!(out.get("topic"), Some(&json!("dbl")));
    assert!(out.get("_msgid").is_some());
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn compile_error_emits_on_err_port() {
    // Unbalanced brace — compile-stage failure.
    let (engine, graph, path) = setup_with_config(json!({
        "script": "msg.payload = {;",
        "on_error": "emit_err",
    }))
    .await;

    fire_in(&graph, &path, Msg::new(json!(1))).await;

    // Nothing on `out`.
    assert!(
        slot(&graph, &path, "out").get("_msgid").is_none(),
        "compile error must not emit on out"
    );
    // Error envelope on `err`.
    let err = slot(&graph, &path, "err");
    assert_eq!(err.get("payload").and_then(|p| p.get("stage")), Some(&json!("compile")));
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn max_operations_trips_on_runaway_loop() {
    // Tight infinite loop — must hit the op counter and surface as a runtime error.
    let (engine, graph, path) = setup_with_config(json!({
        "script": "loop { }",
        "max_operations": 1000,
        "on_error": "emit_err",
    }))
    .await;

    fire_in(&graph, &path, Msg::new(json!(1))).await;
    let err = slot(&graph, &path, "err");
    assert_eq!(
        err.get("payload").and_then(|p| p.get("stage")),
        Some(&json!("runtime")),
        "op-limit trip should classify as runtime: {err}"
    );
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn counters_bump_on_success_and_error() {
    let (engine, graph, path) = setup_with_config(json!({
        "script": "if msg.payload == 0 { throw \"boom\" } else { msg }",
        "on_error": "drop",
    }))
    .await;

    fire_in(&graph, &path, Msg::new(json!(1))).await;
    fire_in(&graph, &path, Msg::new(json!(2))).await;
    fire_in(&graph, &path, Msg::new(json!(0))).await;

    assert_eq!(slot(&graph, &path, "exec_count"), json!(3));
    assert_eq!(slot(&graph, &path, "error_count"), json!(1));
    engine.shutdown().await.unwrap();
}
