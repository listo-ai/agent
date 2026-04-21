//! End-to-end: two Zenoh peers in one process, one serves the agent's
//! fleet surface, the other issues a real `request` for
//! `api.v1.search` (scope=nodes) and validates the reply.
//!
//! This is the "edge → cloud over the wire" smoke test for the whole
//! seam:
//!
//!   request  →  ZenohTransport (peer B)
//!                    ↓  real zenoh query
//!            ZenohTransport (peer A) queryable
//!                    ↓  FleetHandler dispatch
//!            NodesScope::query_page(NodesQuery)
//!                    ↓  JSON encode
//!   reply   ←  back down the wire

use std::sync::Arc;
use std::time::Duration;

use blocks_host::BlockRegistry;
use engine::BehaviorRegistry;
use graph::{seed as graph_seed, GraphStore, KindRegistry, NullSink};
use spi::{FleetTransport, KindId, NodePath, Subject, TenantId};
use tokio::sync::broadcast;
use transport_fleet_zenoh::{ZenohConfig, ZenohTransport};
use transport_rest::AppState;

fn make_state_with_fleet(fleet: Arc<dyn FleetTransport>) -> AppState {
    let kinds = KindRegistry::new();
    graph_seed::register_builtins(&kinds);
    let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
    graph.create_root(KindId::new("sys.core.station")).unwrap();
    graph
        .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "alpha")
        .unwrap();
    graph
        .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "beta")
        .unwrap();

    let (events, _) = broadcast::channel(16);
    let (behaviors, _) = BehaviorRegistry::new(graph.clone());
    AppState::new(graph, behaviors, events, BlockRegistry::new()).with_fleet(fleet)
}

/// Run two Zenoh peers on loopback: `edge` mounts the fleet handlers,
/// `cloud` requests `api.v1.search` with `scope=nodes` and checks the
/// reply.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opens real zenoh sessions on loopback; run with `cargo test --test fleet_zenoh_e2e -- --ignored`"]
async fn search_nodes_roundtrip_over_zenoh() {
    let tenant = TenantId::new("sys");
    let agent_id = "edge-1";

    // Peer A — the "edge": listens, mounts fleet handlers.
    let edge = Arc::new(
        ZenohTransport::connect(ZenohConfig {
            listen: vec!["tcp/127.0.0.1:17447".to_string()],
            connect: vec![],
        })
        .await
        .expect("edge connect"),
    );
    let edge_state = make_state_with_fleet(edge.clone());

    // Mount the fleet surface — `api.v1.search` handles the unified
    // search RPC on `fleet.sys.edge-1.api.v1.search`.
    let _servers = transport_rest::fleet::mount(edge_state.clone(), &tenant, agent_id)
        .await
        .expect("mount edge fleet handlers");

    // Peer B — the "cloud": connects to edge's listen address.
    let cloud = ZenohTransport::connect(ZenohConfig {
        listen: vec![],
        connect: vec!["tcp/127.0.0.1:17447".to_string()],
    })
    .await
    .expect("cloud connect");

    // Zenoh discovery takes a moment to propagate a queryable.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Cloud asks the edge for its node list via the generic search
    // subject. Payload carries the scope and any filter/sort params.
    let subj = Subject::for_agent(&tenant, agent_id)
        .kind("api.v1.search")
        .build();
    let payload = serde_json::to_vec(&serde_json::json!({ "scope": "nodes" }))
        .expect("encode search payload");
    let reply = cloud
        .request(&subj, payload, Duration::from_secs(3))
        .await
        .expect("round-trip reply");

    let parsed: serde_json::Value = serde_json::from_slice(&reply).expect("reply is json");

    // Shape check — search envelope `{ scope, hits, meta }` with 3
    // nodes (root + alpha + beta) that `make_state_with_fleet` seeds.
    assert_eq!(parsed.get("scope").and_then(|s| s.as_str()), Some("nodes"));
    let hits = parsed.get("hits").expect("reply has hits");
    let arr = hits.as_array().expect("hits is an array");
    let paths: Vec<&str> = arr
        .iter()
        .filter_map(|n| n.get("path").and_then(|p| p.as_str()))
        .collect();
    assert!(paths.contains(&"/"), "root present: {paths:?}");
    assert!(paths.contains(&"/alpha"), "alpha present: {paths:?}");
    assert!(paths.contains(&"/beta"), "beta present: {paths:?}");
}
