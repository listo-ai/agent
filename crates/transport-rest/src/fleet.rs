//! Fleet-side mounting of the REST surface.
//!
//! Registers the same core handler functions that the axum router
//! exposes over HTTP, but on `fleet.<tenant>.<agent-id>.api.v1.*`
//! subjects. One internal fn per route, two transports serving it —
//! see `docs/design/FLEET-TRANSPORT.md` § "Studio's transport abstraction".
//!
//! Mounted subjects (all under `fleet.<tenant>.<agent-id>`):
//!
//! | Subject suffix             | HTTP equivalent              |
//! |----------------------------|------------------------------|
//! | `api.v1.nodes.list`        | GET  /api/v1/nodes           |
//! | `api.v1.nodes.get`         | GET  /api/v1/node            |
//! | `api.v1.slots.write`       | POST /api/v1/slots           |

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use spi::{FleetError, FleetHandler, FleetMessage, Payload, Server, Subject, TenantId};

use crate::routes::{
    get_node_core, list_nodes_core, write_slot_core, ListNodesQuery, WriteSlotReq,
};
use crate::state::AppState;

/// Register every fleet handler for this agent. Returns the collection
/// of `Server` handles — drop them to deregister.
///
/// The caller supplies the agent's `(tenant, agent_id)` pair; these
/// become the prefix `fleet.<tenant>.<agent-id>.*` under which every
/// subject lives, per the canonical namespace in
/// `docs/design/FLEET-TRANSPORT.md` § "Subject namespace".
pub async fn mount(
    state: AppState,
    tenant: &TenantId,
    agent_id: &str,
) -> Result<Vec<Server>, FleetError> {
    let fleet = state.fleet.clone();
    let mut servers = Vec::new();

    // api.v1.nodes.list — mirrors GET /api/v1/nodes
    let list_nodes_subj = Subject::for_agent(tenant, agent_id)
        .kind("api.v1.nodes.list")
        .build();
    servers.push(
        fleet
            .serve(
                &list_nodes_subj,
                Arc::new(ListNodesHandler {
                    state: state.clone(),
                }),
            )
            .await?,
    );

    // api.v1.nodes.get — mirrors GET /api/v1/node?path=...
    let get_node_subj = Subject::for_agent(tenant, agent_id)
        .kind("api.v1.nodes.get")
        .build();
    servers.push(
        fleet
            .serve(
                &get_node_subj,
                Arc::new(GetNodeHandler {
                    state: state.clone(),
                }),
            )
            .await?,
    );

    // api.v1.slots.write — mirrors POST /api/v1/slots
    let write_slot_subj = Subject::for_agent(tenant, agent_id)
        .kind("api.v1.slots.write")
        .build();
    servers.push(
        fleet
            .serve(
                &write_slot_subj,
                Arc::new(WriteSlotHandler {
                    state: state.clone(),
                }),
            )
            .await?,
    );

    Ok(servers)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Fleet handler for `api.v1.nodes.list`.
///
/// Payload: optional JSON-encoded [`ListNodesQuery`] (empty payload
/// uses defaults). Reply: JSON-encoded `Page<NodeDto>`.
struct ListNodesHandler {
    state: AppState,
}

impl FleetHandler for ListNodesHandler {
    fn handle<'a>(
        &'a self,
        msg: FleetMessage,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Payload>, FleetError>> + Send + 'a>> {
        Box::pin(async move {
            let req: ListNodesQuery = if msg.payload.is_empty() {
                ListNodesQuery::default()
            } else {
                serde_json::from_slice(&msg.payload).map_err(|e| FleetError::InvalidSubject {
                    reason: format!("request body not valid ListNodesQuery JSON: {e}"),
                })?
            };

            let page = list_nodes_core(&self.state, req)
                .map_err(|e| FleetError::Backend(format!("list_nodes: {e:?}")))?;

            let bytes = serde_json::to_vec(&page)
                .map_err(|e| FleetError::Backend(format!("encode reply: {e}")))?;
            Ok(Some(bytes))
        })
    }
}

/// Fleet handler for `api.v1.nodes.get`.
///
/// Payload: `{ "path": "/some/path", "include_internal": false }`.
/// Reply: JSON-encoded `NodeDto` or a `{ "code": "not_found" }` error.
struct GetNodeHandler {
    state: AppState,
}

#[derive(serde::Deserialize)]
struct GetNodeFleetReq {
    path: String,
    #[serde(default)]
    include_internal: bool,
}

impl FleetHandler for GetNodeHandler {
    fn handle<'a>(
        &'a self,
        msg: FleetMessage,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Payload>, FleetError>> + Send + 'a>> {
        Box::pin(async move {
            let req: GetNodeFleetReq =
                serde_json::from_slice(&msg.payload).map_err(|e| FleetError::InvalidSubject {
                    reason: format!("request body not valid GetNode JSON: {e}"),
                })?;

            let dto = get_node_core(&self.state, &req.path, req.include_internal)
                .map_err(|e| FleetError::Backend(format!("get_node: {e:?}")))?;

            let bytes = serde_json::to_vec(&dto)
                .map_err(|e| FleetError::Backend(format!("encode reply: {e}")))?;
            Ok(Some(bytes))
        })
    }
}

/// Fleet handler for `api.v1.slots.write`.
///
/// Payload: JSON-encoded [`WriteSlotReq`] (`{ path, slot, value,
/// expected_generation? }`).
///
/// Reply: `{ "status": "ok", "generation": N }` on success, or
/// `{ "status": "generation_mismatch", "current_generation": N }` on
/// a CAS conflict. The status-tagged shape avoids HTTP status codes on
/// the fleet path while remaining unambiguous.
///
/// Auth is the caller's responsibility — bearer tokens are forwarded as
/// fleet message headers and must be validated before this handler is
/// reached (tracked in `docs/design/FLEET-TRANSPORT.md` gap #6).
struct WriteSlotHandler {
    state: AppState,
}

impl FleetHandler for WriteSlotHandler {
    fn handle<'a>(
        &'a self,
        msg: FleetMessage,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Payload>, FleetError>> + Send + 'a>> {
        Box::pin(async move {
            let req: WriteSlotReq =
                serde_json::from_slice(&msg.payload).map_err(|e| FleetError::InvalidSubject {
                    reason: format!("request body not valid WriteSlot JSON: {e}"),
                })?;

            let result = write_slot_core(&self.state, req)
                .map_err(|e| FleetError::Backend(format!("write_slot: {e:?}")))?;

            // Both Ok and GenerationMismatch are successful fleet replies —
            // the status tag tells the caller which outcome occurred.
            let bytes = serde_json::to_vec(&result)
                .map_err(|e| FleetError::Backend(format!("encode reply: {e}")))?;
            Ok(Some(bytes))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use blocks_host::BlockRegistry;
    use engine::BehaviorRegistry;
    use graph::{seed as graph_seed, GraphStore, KindRegistry, NullSink};
    use spi::{KindId, NodePath};
    use tokio::sync::broadcast;

    fn state() -> AppState {
        let kinds = KindRegistry::new();
        graph_seed::register_builtins(&kinds);
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "alpha")
            .unwrap();
        let (events, _) = broadcast::channel(16);
        let (behaviors, _) = BehaviorRegistry::new(graph.clone());
        AppState::new(graph, behaviors, events, BlockRegistry::new())
    }

    /// The fleet handler and HTTP handler use the same core fn — verify
    /// that invoking it through the fleet seam returns the same payload
    /// shape as the HTTP route.
    #[tokio::test]
    async fn fleet_list_nodes_returns_same_shape_as_http() {
        let s = state();

        // Call the core fn directly (what the axum handler does).
        let direct = list_nodes_core(&s, ListNodesQuery::default()).unwrap();
        let direct_json = serde_json::to_value(&direct).unwrap();

        // Call it through the FleetHandler (what the fleet mount does).
        let handler = ListNodesHandler { state: s };
        let msg = FleetMessage {
            subject: Subject::for_agent(&TenantId::default_tenant(), "edge-1")
                .kind("api.v1.nodes.list")
                .build(),
            payload: vec![],
            reply_to: None,
        };
        let reply = handler.handle(msg).await.unwrap().unwrap();
        let fleet_json: serde_json::Value = serde_json::from_slice(&reply).unwrap();

        assert_eq!(
            direct_json, fleet_json,
            "fleet handler reply must be byte-identical to HTTP core fn result",
        );
    }

    #[tokio::test]
    async fn fleet_get_node_returns_same_shape_as_http() {
        let s = state();

        let direct = get_node_core(&s, "/alpha", false).unwrap();
        let direct_json = serde_json::to_value(&direct).unwrap();

        let handler = GetNodeHandler { state: s };
        let msg = FleetMessage {
            subject: Subject::for_agent(&TenantId::default_tenant(), "edge-1")
                .kind("api.v1.nodes.get")
                .build(),
            payload: serde_json::to_vec(&serde_json::json!({ "path": "/alpha" })).unwrap(),
            reply_to: None,
        };
        let reply = handler.handle(msg).await.unwrap().unwrap();
        let fleet_json: serde_json::Value = serde_json::from_slice(&reply).unwrap();

        assert_eq!(
            direct_json, fleet_json,
            "fleet get_node reply must match HTTP core fn result",
        );
    }

    #[tokio::test]
    async fn fleet_write_slot_returns_ok_status() {
        use spi::{
            Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindManifest, SlotRole,
            SlotSchema,
        };

        // Register a minimal test kind with one writable slot.
        let kinds = KindRegistry::new();
        graph_seed::register_builtins(&kinds);
        kinds.register(KindManifest {
            id: KindId::new("test.fleet.node"),
            display_name: None,
            facets: FacetSet::of([Facet::IsCompute]),
            containment: ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
            slots: vec![SlotSchema::new("value", SlotRole::Status).writable()],
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        });
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("test.fleet.node"), "n1")
            .unwrap();
        let (events, _) = broadcast::channel(16);
        let (behaviors, _) = BehaviorRegistry::new(graph.clone());
        let s = AppState::new(graph, behaviors, events, BlockRegistry::new());

        let handler = WriteSlotHandler { state: s };
        let msg = FleetMessage {
            subject: Subject::for_agent(&TenantId::default_tenant(), "edge-1")
                .kind("api.v1.slots.write")
                .build(),
            payload: serde_json::to_vec(&serde_json::json!({
                "path": "/n1",
                "slot": "value",
                "value": 42
            }))
            .unwrap(),
            reply_to: None,
        };
        let reply = handler.handle(msg).await.unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(
            v["status"], "ok",
            "write_slot fleet reply must have status=ok on success"
        );
        assert!(
            v["generation"].is_number(),
            "write_slot fleet reply must include generation"
        );
    }
}
