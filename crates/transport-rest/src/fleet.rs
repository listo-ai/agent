//! Fleet-side mounting of the REST surface.
//!
//! Registers the same core handler functions that the axum router
//! exposes over HTTP, but on `fleet.<tenant>.<agent-id>.api.v1.*`
//! subjects. One internal fn per route, two transports serving it —
//! see `docs/design/FLEET-TRANSPORT.md` § "Studio's transport abstraction".
//!
//! Today only `api.v1.nodes.list` is wired through the seam. More
//! routes follow the same pattern; keep the shared core fns in
//! [`crate::routes`] so drift between HTTP and fleet is impossible.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use spi::{FleetError, FleetHandler, FleetMessage, Payload, Server, Subject, TenantId};

use crate::routes::{list_nodes_core, ListNodesQuery};
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

    let list_nodes_subj = Subject::for_agent(tenant, agent_id)
        .kind("api.v1.nodes.list")
        .build();

    let handler: Arc<dyn FleetHandler> = Arc::new(ListNodesHandler { state });
    let server = fleet.serve(&list_nodes_subj, handler).await?;
    servers.push(server);

    Ok(servers)
}

/// Fleet handler for `api.v1.nodes.list`. Decodes the request from the
/// payload (JSON-encoded `ListNodesQuery`), calls the shared core fn,
/// encodes the reply as JSON.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use engine::BehaviorRegistry;
    use extensions_host::PluginRegistry;
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
        AppState::new(graph, behaviors, events, PluginRegistry::new())
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

        assert_eq!(direct_json, fleet_json);
    }

    #[tokio::test]
    async fn mount_on_null_transport_reports_disabled() {
        let s = state();
        let err = mount(s, &TenantId::default_tenant(), "edge-1")
            .await
            .unwrap_err();
        assert!(matches!(err, FleetError::Disabled));
    }
}
