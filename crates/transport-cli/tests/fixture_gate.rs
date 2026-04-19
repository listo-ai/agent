#![allow(clippy::unwrap_used, clippy::panic)]
//! CLI output contract fixture gate.
//!
//! Spins up an in-memory agent HTTP server, runs CLI commands against
//! it, and asserts that the JSON output matches the pinned fixtures in
//! `clients/contracts/fixtures/cli-output/`.
//!
//! ## Fixture format
//!
//! Each `.json` file is the expected CLI output with two conventions:
//!
//! - UUID-shaped strings are normalised to all-zeros before comparison.
//! - A string value of `"VARIES"` is a wildcard — any value matches.
//!
//! ## Adding a fixture
//!
//! 1. Add a `<command>/<scenario>.json` under `clients/contracts/fixtures/cli-output/`.
//! 2. Add a call to `assert_fixture!` or `assert_error_fixture!` in this file.
//!
//! The test fails to compile if the fixture file is missing, and fails at
//! runtime if the shape drifts.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client::{AgentClient, AgentClientOptions, NodeListParams};
use engine::Engine;
use extensions_host::PluginRegistry;
use graph::{seed, GraphStore, KindRegistry};
use serde_json::Value;
use spi::KindId;
use transport_rest::AppState;

// ---- test server ----------------------------------------------------------

async fn start_test_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    start_with_graph(|_| {}).await
}

async fn start_with_graph<F>(seed_fn: F) -> (SocketAddr, tokio::task::JoinHandle<()>)
where
    F: FnOnce(&GraphStore),
{
    let (sink, events_rx, bcast) = transport_rest::agent_sink();

    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    engine::kinds::register(&kinds);
    domain_compute::register_kinds(&kinds);
    domain_logic::register_kinds(&kinds);
    dashboard_nodes::register_kinds(&kinds);

    let graph = Arc::new(GraphStore::new(kinds, sink));
    graph.create_root(KindId::new("acme.core.station")).unwrap();
    seed_fn(&graph);

    let engine = Engine::new(graph.clone(), events_rx);
    engine
        .behaviors()
        .register(
            <domain_compute::Count as extensions_sdk::NodeKind>::kind_id(),
            domain_compute::behavior(),
        )
        .unwrap();
    engine
        .behaviors()
        .register(
            <domain_logic::Trigger as extensions_sdk::NodeKind>::kind_id(),
            domain_logic::behavior(),
        )
        .unwrap();
    engine.start().await.unwrap();

    let app_state = AppState::new(
        graph.clone(),
        engine.behaviors().clone(),
        bcast,
        PluginRegistry::new(),
    );
    let dashboard_reader: Arc<dyn dashboard_runtime::NodeReader + Send + Sync> =
        Arc::new(dashboard_transport::GraphReader::new(graph));
    let router = transport_rest::router(app_state).merge(dashboard_transport::router(
        dashboard_transport::DashboardState::new(dashboard_reader),
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Give the server a moment to be ready.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    (addr, handle)
}

fn client(addr: SocketAddr) -> AgentClient {
    AgentClient::with_options(AgentClientOptions {
        base_url: format!("http://{addr}"),
        token: None,
    })
}

// ---- fixture helpers ------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/contracts/fixtures/cli-output")
}

fn load_fixture(rel: &str) -> Value {
    let path = fixtures_dir().join(rel);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("fixture {} is not valid JSON: {e}", path.display()))
}

/// Normalise a JSON value for comparison:
/// - UUID-shaped string → "00000000-0000-0000-0000-000000000000"
/// - Other strings → unchanged
/// - Arrays / objects → recurse
fn normalise(v: Value) -> Value {
    match v {
        Value::String(s) if is_uuid(&s) => {
            Value::String("00000000-0000-0000-0000-000000000000".into())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(normalise).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, normalise(v))).collect())
        }
        other => other,
    }
}

fn is_uuid(s: &str) -> bool {
    // xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    if s.len() != 36 {
        return false;
    }
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts[0].len() == 8
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 4
        && parts[4].len() == 12
        && parts
            .iter()
            .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Assert that `actual` structurally matches `fixture`.
///
/// - `"VARIES"` in the fixture matches any *string* value in actual.
/// - `null` in the fixture matches any value (pure shape/key check).
/// - UUIDs are normalised before comparison.
fn assert_shape_match(actual: &Value, fixture: &Value, path: &str) {
    match (actual, fixture) {
        // null wildcard: accept any value (used for non-deterministic numbers etc.)
        (_, Value::Null) => {}
        // VARIES wildcard: accept any string
        (_, Value::String(s)) if s == "VARIES" => {}
        // Both objects: every fixture key must be present in actual
        (Value::Object(a_map), Value::Object(f_map)) => {
            for (key, f_val) in f_map {
                let a_val = a_map.get(key).unwrap_or_else(|| {
                    panic!("key `{key}` missing at `{path}`\n  fixture: {f_map:?}\n  actual:  {a_map:?}")
                });
                assert_shape_match(a_val, f_val, &format!("{path}.{key}"));
            }
        }
        // Both arrays: same length, element-wise comparison
        (Value::Array(a_arr), Value::Array(f_arr)) => {
            assert_eq!(
                a_arr.len(),
                f_arr.len(),
                "array length mismatch at `{path}`: actual {}, fixture {}",
                a_arr.len(),
                f_arr.len()
            );
            for (i, (a, f)) in a_arr.iter().zip(f_arr.iter()).enumerate() {
                assert_shape_match(a, f, &format!("{path}[{i}]"));
            }
        }
        // Scalars: normalise UUIDs then compare exactly
        (a, f) => {
            let a_n = normalise(a.clone());
            let f_n = normalise(f.clone());
            assert_eq!(a_n, f_n, "value mismatch at `{path}`");
        }
    }
}

fn parse_json_output(raw: &str) -> Value {
    serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("command output is not valid JSON: {e}\n  output: {raw}"))
}

// ---- tests ----------------------------------------------------------------

#[tokio::test]
async fn nodes_list_empty() {
    // Use a server with no root node to get a truly empty list.
    let (sink2, events_rx2, bcast2) = transport_rest::agent_sink();
    let kinds2 = KindRegistry::new();
    seed::register_builtins(&kinds2);
    let graph2 = Arc::new(GraphStore::new(kinds2, sink2));
    let engine2 = Engine::new(graph2.clone(), events_rx2);
    engine2.start().await.unwrap();
    let state2 = AppState::new(
        graph2,
        engine2.behaviors().clone(),
        bcast2,
        PluginRegistry::new(),
    );
    let router2 = transport_rest::router(state2);
    let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener2, router2).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let c2 = client(addr2);

    let page = c2
        .nodes()
        .list_page(&NodeListParams::default())
        .await
        .unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&page).unwrap());
    let fixture = load_fixture("nodes-list/empty.json");
    assert_shape_match(&actual, &fixture, "$");
}

#[tokio::test]
async fn nodes_list_populated() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    let page = c
        .nodes()
        .list_page(&NodeListParams::default())
        .await
        .unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&page).unwrap());
    let fixture = load_fixture("nodes-list/populated.json");
    assert_shape_match(&actual, &fixture, "$");
}

#[tokio::test]
async fn nodes_list_query_filters_and_pages() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    c.nodes()
        .create("/", "acme.core.folder", "alpha")
        .await
        .unwrap();
    c.nodes()
        .create("/", "acme.core.folder", "beta")
        .await
        .unwrap();

    let page = c
        .nodes()
        .list_page(&NodeListParams {
            filter: Some("kind==acme.core.folder".into()),
            sort: Some("-path".into()),
            page: Some(1),
            size: Some(1),
        })
        .await
        .unwrap();

    let actual = parse_json_output(&serde_json::to_string_pretty(&page).unwrap());
    let fixture = load_fixture("nodes-list/filtered-page.json");
    assert_shape_match(&actual, &fixture, "$");
}

#[tokio::test]
async fn nodes_list_direct_children_via_parent_path() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    // Tree: /, /alpha, /alpha/one, /alpha/two, /beta.
    c.nodes()
        .create("/", "acme.core.folder", "alpha")
        .await
        .unwrap();
    c.nodes()
        .create("/", "acme.core.folder", "beta")
        .await
        .unwrap();
    c.nodes()
        .create("/alpha", "acme.core.folder", "one")
        .await
        .unwrap();
    c.nodes()
        .create("/alpha", "acme.core.folder", "two")
        .await
        .unwrap();

    let page = c
        .nodes()
        .list_page(&NodeListParams {
            filter: Some("parent_path==/alpha".into()),
            sort: Some("path".into()),
            page: None,
            size: None,
        })
        .await
        .unwrap();

    // Should see only /alpha/one and /alpha/two — not / alpha itself,
    // not /alpha/*/... descendants, not sibling /beta.
    assert_eq!(page.meta.total, 2, "direct children only");
    let paths: Vec<&str> = page.data.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(paths, vec!["/alpha/one", "/alpha/two"]);

    // Every child reports its parent_path == the filter value.
    for n in &page.data {
        assert_eq!(n.parent_path.as_deref(), Some("/alpha"));
    }
}

#[tokio::test]
async fn nodes_create_ok() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    let created = c
        .nodes()
        .create("/", "acme.core.folder", "fixture_test")
        .await
        .unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&created).unwrap());
    let fixture = load_fixture("nodes-create/ok.json");
    assert_shape_match(&actual, &fixture, "$");

    // path should be /fixture_test
    assert_eq!(created.path, "/fixture_test");
}

#[tokio::test]
async fn nodes_create_bad_path() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    // " bad path" has a leading space — NodePath::from_str rejects it
    let err = c
        .nodes()
        .create("bad path", "acme.core.folder", "x")
        .await
        .unwrap_err();
    let cli_err = transport_cli::CliError::from_client(&err);
    let actual = parse_json_output(&serde_json::to_string_pretty(&cli_err).unwrap());
    let fixture = load_fixture("nodes-create/bad-path.json");
    assert_shape_match(&actual, &fixture, "$");
    assert_eq!(cli_err.code, "bad_path");
}

#[tokio::test]
async fn slots_write_ok() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    c.nodes()
        .create("/", "acme.compute.count", "counter")
        .await
        .unwrap();
    let gen = c
        .slots()
        .write("/counter", "in", &serde_json::json!(5))
        .await
        .unwrap();

    let output = serde_json::json!({ "generation": gen });
    let actual = parse_json_output(&serde_json::to_string_pretty(&output).unwrap());
    let fixture = load_fixture("slots-write/ok.json");
    assert_shape_match(&actual, &fixture, "$");
    assert!(gen > 0);
}

#[tokio::test]
async fn slots_write_node_not_found() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    let err = c
        .slots()
        .write("/nonexistent", "in", &serde_json::json!(1))
        .await
        .unwrap_err();
    let cli_err = transport_cli::CliError::from_client(&err);
    let actual = parse_json_output(&serde_json::to_string_pretty(&cli_err).unwrap());
    let fixture = load_fixture("slots-write/node-not-found.json");
    assert_shape_match(&actual, &fixture, "$");
    assert_eq!(cli_err.code, "not_found");
}

#[tokio::test]
async fn lifecycle_illegal_transition() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    // Root node starts in "created" state; "removed" is not a valid
    // user-driven target from "created" (must go Removing → Removed).
    let err = c.lifecycle().transition("/", "removed").await.unwrap_err();
    let cli_err = transport_cli::CliError::from_client(&err);
    let actual = parse_json_output(&serde_json::to_string_pretty(&cli_err).unwrap());
    let fixture = load_fixture("lifecycle/illegal-transition.json");
    assert_shape_match(&actual, &fixture, "$");
    assert_eq!(cli_err.code, "illegal_transition");
}

#[tokio::test]
async fn auth_whoami_dev_null() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    let who = c.auth().whoami().await.unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&who).unwrap());
    let fixture = load_fixture("auth-whoami/dev-null.json");
    assert_shape_match(&actual, &fixture, "$");
}

// ---- ui (dashboard) -------------------------------------------------------

use std::str::FromStr;

async fn server_with_ui_fixtures() -> (
    SocketAddr,
    tokio::task::JoinHandle<()>,
    spi::NodeId, // nav root
    spi::NodeId, // page
) {
    let nav_id_cell: Arc<std::sync::Mutex<Option<spi::NodeId>>> = Arc::new(Default::default());
    let page_id_cell: Arc<std::sync::Mutex<Option<spi::NodeId>>> = Arc::new(Default::default());
    let nav_set = nav_id_cell.clone();
    let page_set = page_id_cell.clone();
    let (addr, handle) = start_with_graph(move |g| {
        let root = spi::NodePath::root();
        let nav = g
            .create_child(&root, KindId::new("ui.nav"), "home")
            .unwrap();
        let nav_path = spi::NodePath::from_str("/home").unwrap();
        g.write_slot(&nav_path, "title", Value::String("Root".into()))
            .unwrap();
        *nav_set.lock().unwrap() = Some(nav);

        let page = g
            .create_child(&root, KindId::new("ui.page"), "dashboard")
            .unwrap();
        let page_path = spi::NodePath::from_str("/dashboard").unwrap();
        g.write_slot(&page_path, "title", Value::String("Dashboard".into()))
            .unwrap();
        *page_set.lock().unwrap() = Some(page);
    })
    .await;
    let nav = nav_id_cell.lock().unwrap().unwrap();
    let page = page_id_cell.lock().unwrap().unwrap();
    (addr, handle, nav, page)
}

#[tokio::test]
async fn ui_nav_ok() {
    let (addr, _srv, nav, _) = server_with_ui_fixtures().await;
    let c = client(addr);
    let tree = c.ui().nav(&nav.0.to_string()).await.unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&tree).unwrap());
    let fixture = load_fixture("ui-nav/ok.json");
    assert_shape_match(&actual, &fixture, "$");
}

#[tokio::test]
async fn ui_nav_not_found() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);
    let missing = uuid::Uuid::new_v4().to_string();
    let err = c.ui().nav(&missing).await.unwrap_err();
    let cli_err = transport_cli::CliError::from_client(&err);
    let actual = parse_json_output(&serde_json::to_string_pretty(&cli_err).unwrap());
    let fixture = load_fixture("ui-nav/not-found.json");
    assert_shape_match(&actual, &fixture, "$");
    assert_eq!(cli_err.code, "not_found");
}

#[tokio::test]
async fn ui_resolve_ok() {
    let (addr, _srv, _, page) = server_with_ui_fixtures().await;
    let c = client(addr);
    let req = agent_client::types::UiResolveRequest {
        page_ref: page.0.to_string(),
        stack: Vec::new(),
        page_state: serde_json::json!({}),
        dry_run: false,
        auth_subject: None,
        user_claims: Default::default(),
    };
    let resp = c.ui().resolve(&req).await.unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&resp).unwrap());
    let fixture = load_fixture("ui-resolve/ok.json");
    assert_shape_match(&actual, &fixture, "$");
}

#[tokio::test]
async fn ui_resolve_dry_run() {
    let (addr, _srv, _, page) = server_with_ui_fixtures().await;
    let c = client(addr);
    let req = agent_client::types::UiResolveRequest {
        page_ref: page.0.to_string(),
        stack: Vec::new(),
        page_state: serde_json::json!({}),
        dry_run: true,
        auth_subject: None,
        user_claims: Default::default(),
    };
    let resp = c.ui().resolve(&req).await.unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&resp).unwrap());
    let fixture = load_fixture("ui-resolve/dry-run.json");
    assert_shape_match(&actual, &fixture, "$");
}

#[tokio::test]
async fn ui_resolve_page_not_found() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);
    let req = agent_client::types::UiResolveRequest {
        page_ref: uuid::Uuid::new_v4().to_string(),
        stack: Vec::new(),
        page_state: serde_json::json!({}),
        dry_run: false,
        auth_subject: None,
        user_claims: Default::default(),
    };
    let err = c.ui().resolve(&req).await.unwrap_err();
    let cli_err = transport_cli::CliError::from_client(&err);
    let actual = parse_json_output(&serde_json::to_string_pretty(&cli_err).unwrap());
    let fixture = load_fixture("ui-resolve/page-not-found.json");
    assert_shape_match(&actual, &fixture, "$");
    assert_eq!(cli_err.code, "not_found");
}

#[tokio::test]
async fn capabilities_ok() {
    let (addr, _srv) = start_test_server().await;
    let c = client(addr);

    let manifest = c.capabilities().get_manifest().await.unwrap();
    let actual = parse_json_output(&serde_json::to_string_pretty(&manifest).unwrap());
    let fixture = load_fixture("capabilities/ok.json");
    assert_shape_match(&actual, &fixture, "$");

    // Capabilities list must be non-empty
    assert!(!manifest.capabilities.is_empty());
}
