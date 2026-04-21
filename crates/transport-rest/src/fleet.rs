//! Fleet-side mounting of the REST surface.
//!
//! Registers the same core handler functions that the axum router
//! exposes over HTTP, but on `fleet.<tenant>.<agent-id>.api.v1.*`
//! subjects. One internal fn per route, two transports serving it —
//! see `docs/design/FLEET-TRANSPORT.md` § "Studio's transport abstraction".
//!
//! Mounted subjects (all under `fleet.<tenant>.<agent-id>`):
//!
//! | Subject suffix             | HTTP equivalent                       |
//! |----------------------------|---------------------------------------|
//! | `api.v1.search`            | GET  /api/v1/search                   |
//! | `api.v1.nodes.get`         | GET  /api/v1/node                     |
//! | `api.v1.slots.write`       | POST /api/v1/slots                    |

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use blocks_host::{BlocksQuery, BlocksScope};
use domain_flows::{FlowsQuery, FlowsScope};
use graph::{
    KindsQuery, KindsScope, LinksQuery, LinksScope, NodesQuery, NodesScope, SearchScope,
};
use serde::{Deserialize, Serialize};
use spi::{Facet, FleetError, FleetHandler, FleetMessage, NodePath, Payload, Server, Subject, TenantId};

use crate::routes::{get_node_core, write_slot_core, WriteSlotReq};
use crate::state::AppState;

/// Register every fleet handler for this agent. Returns the collection
/// of `Server` handles — drop them to deregister.
pub async fn mount(
    state: AppState,
    tenant: &TenantId,
    agent_id: &str,
) -> Result<Vec<Server>, FleetError> {
    let fleet = state.fleet.clone();
    let mut servers = Vec::new();

    // api.v1.search — mirrors GET /api/v1/search?scope=<id>.
    let search_subj = Subject::for_agent(tenant, agent_id)
        .kind("api.v1.search")
        .build();
    servers.push(
        fleet
            .serve(
                &search_subj,
                Arc::new(SearchHandler {
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
// Search handler — single fleet subject for every scope, mirroring HTTP.
// ---------------------------------------------------------------------------

/// Wire shape for `api.v1.search` — identical to the REST query params
/// accepted by `GET /api/v1/search`.
#[derive(Debug, Default, Deserialize)]
struct SearchReq {
    scope: String,
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    facet: Option<String>,
    #[serde(default)]
    placeable_under: Option<String>,
    #[serde(default)]
    page: Option<usize>,
    #[serde(default)]
    size: Option<usize>,
}

/// Reply envelope — same shape the REST handler emits so the two
/// surfaces round-trip identically. Hits are stored as `serde_json::Value`
/// to carry any scope's row shape without propagating a generic type
/// through the dyn `FleetHandler` trait.
#[derive(Debug, Serialize)]
struct SearchReply {
    scope: &'static str,
    hits: Vec<serde_json::Value>,
    meta: SearchMeta,
}

#[derive(Debug, Default, Serialize)]
struct SearchMeta {
    total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pages: Option<usize>,
}

struct SearchHandler {
    state: AppState,
}

impl FleetHandler for SearchHandler {
    fn handle<'a>(
        &'a self,
        msg: FleetMessage,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Payload>, FleetError>> + Send + 'a>> {
        Box::pin(async move {
            let req: SearchReq = if msg.payload.is_empty() {
                return Err(FleetError::InvalidSubject {
                    reason: "search payload is required (needs `scope`)".into(),
                });
            } else {
                serde_json::from_slice(&msg.payload).map_err(|e| FleetError::InvalidSubject {
                    reason: format!("request body not valid search JSON: {e}"),
                })?
            };

            let reply = match req.scope.as_str() {
                "kinds" => {
                    let facet = parse_facet(&req.facet)?;
                    let placeable_under = parse_path(&req.placeable_under)?;
                    let scope = KindsScope::new(&self.state.graph);
                    let hits = scope
                        .query(KindsQuery {
                            facet,
                            placeable_under,
                            filter: req.filter,
                            sort: req.sort,
                        })
                        .map_err(|e| FleetError::Backend(format!("search(kinds): {e:?}")))?;
                    SearchReply {
                        scope: "kinds",
                        hits: hits_to_json(hits.data)?,
                        meta: SearchMeta {
                            total: hits.total,
                            ..Default::default()
                        },
                    }
                }
                "nodes" => {
                    let scope = NodesScope::new(&self.state.graph);
                    let page = scope
                        .query_page(NodesQuery {
                            filter: req.filter,
                            sort: req.sort,
                            page: req.page,
                            size: req.size,
                        })
                        .map_err(|e| FleetError::Backend(format!("search(nodes): {e:?}")))?;
                    SearchReply {
                        scope: "nodes",
                        hits: hits_to_json(page.data)?,
                        meta: SearchMeta {
                            total: page.meta.total,
                            page: Some(page.meta.page),
                            size: Some(page.meta.size),
                            pages: Some(page.meta.pages),
                        },
                    }
                }
                "blocks" => {
                    let scope = BlocksScope::new(&self.state.blocks);
                    let hits = scope
                        .query(BlocksQuery {
                            filter: req.filter,
                            sort: req.sort,
                        })
                        .map_err(|e| FleetError::Backend(format!("search(blocks): {e:?}")))?;
                    SearchReply {
                        scope: "blocks",
                        hits: hits_to_json(hits.data)?,
                        meta: SearchMeta {
                            total: hits.total,
                            ..Default::default()
                        },
                    }
                }
                "links" => {
                    let scope = LinksScope::new(&self.state.graph);
                    let hits = scope
                        .query(LinksQuery {
                            filter: req.filter,
                            sort: req.sort,
                        })
                        .map_err(|e| FleetError::Backend(format!("search(links): {e:?}")))?;
                    SearchReply {
                        scope: "links",
                        hits: hits_to_json(hits.data)?,
                        meta: SearchMeta {
                            total: hits.total,
                            ..Default::default()
                        },
                    }
                }
                "flows" => {
                    let svc = self.state.flows.as_ref().ok_or_else(|| {
                        FleetError::Backend("flows service not configured".into())
                    })?;
                    let scope = FlowsScope::new(svc);
                    let page = scope
                        .query_page(FlowsQuery {
                            filter: req.filter,
                            sort: req.sort,
                            page: req.page,
                            size: req.size,
                        })
                        .map_err(|e| FleetError::Backend(format!("search(flows): {e:?}")))?;
                    SearchReply {
                        scope: "flows",
                        hits: hits_to_json(page.data)?,
                        meta: SearchMeta {
                            total: page.meta.total,
                            page: Some(page.meta.page),
                            size: Some(page.meta.size),
                            pages: Some(page.meta.pages),
                        },
                    }
                }
                other => {
                    return Err(FleetError::InvalidSubject {
                        reason: format!("unknown scope `{other}`"),
                    });
                }
            };

            let bytes = serde_json::to_vec(&reply)
                .map_err(|e| FleetError::Backend(format!("encode search reply: {e}")))?;
            Ok(Some(bytes))
        })
    }
}

fn parse_facet(raw: &Option<String>) -> Result<Option<Facet>, FleetError> {
    match raw.as_deref() {
        None => Ok(None),
        Some(s) => serde_json::from_str(&format!("\"{s}\""))
            .map(Some)
            .map_err(|_| FleetError::InvalidSubject {
                reason: format!("unknown facet `{s}`"),
            }),
    }
}

fn parse_path(raw: &Option<String>) -> Result<Option<NodePath>, FleetError> {
    use std::str::FromStr;
    match raw.as_deref() {
        None => Ok(None),
        Some(s) => NodePath::from_str(s)
            .map(Some)
            .map_err(|e| FleetError::InvalidSubject {
                reason: format!("bad path `{s}`: {e}"),
            }),
    }
}

fn hits_to_json<T: Serialize>(items: Vec<T>) -> Result<Vec<serde_json::Value>, FleetError> {
    items
        .into_iter()
        .map(|h| serde_json::to_value(&h))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| FleetError::Backend(format!("encode search hit: {e}")))
}

// ---------------------------------------------------------------------------
// Per-node handlers (unchanged)
// ---------------------------------------------------------------------------

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

    /// The fleet search handler and the HTTP `/search` handler share
    /// the same scope implementation — verify the fleet reply shape is
    /// an exact mirror of the REST envelope.
    #[tokio::test]
    async fn fleet_search_nodes_returns_envelope_with_total() {
        let s = state();
        let handler = SearchHandler { state: s };
        let msg = FleetMessage {
            subject: Subject::for_agent(&TenantId::default_tenant(), "edge-1")
                .kind("api.v1.search")
                .build(),
            payload: serde_json::to_vec(&serde_json::json!({ "scope": "nodes" })).unwrap(),
            reply_to: None,
        };
        let reply = handler.handle(msg).await.unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&reply).unwrap();

        assert_eq!(v["scope"], "nodes");
        assert!(v["meta"]["total"].as_u64().unwrap() >= 2);
        assert!(v["hits"].as_array().unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn fleet_search_unknown_scope_errors() {
        let handler = SearchHandler { state: state() };
        let msg = FleetMessage {
            subject: Subject::for_agent(&TenantId::default_tenant(), "edge-1")
                .kind("api.v1.search")
                .build(),
            payload: serde_json::to_vec(&serde_json::json!({ "scope": "bogus" })).unwrap(),
            reply_to: None,
        };
        let err = handler.handle(msg).await.unwrap_err();
        assert!(matches!(err, FleetError::InvalidSubject { .. }));
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

        assert_eq!(direct_json, fleet_json);
    }
}
