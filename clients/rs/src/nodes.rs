//! Node operations — `GET /api/v1/nodes`, `GET /api/v1/node?path=…`,
//! `POST /api/v1/nodes`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{CreatedNode, NodeListResponse, NodeSchema, NodeSnapshot};

use serde::Serialize;

#[derive(Serialize)]
struct CreateNodeReq<'a> {
    parent: &'a str,
    kind: &'a str,
    name: &'a str,
}

pub struct Nodes<'c> {
    http: &'c HttpClient,
    base: String,
}

#[derive(Debug, Clone, Default)]
pub struct NodeListParams {
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub page: Option<u64>,
    pub size: Option<u64>,
}

impl<'c> Nodes<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// List every node in the graph.
    pub async fn list(&self) -> Result<Vec<NodeSnapshot>, ClientError> {
        Ok(self.list_page(&NodeListParams::default()).await?.data)
    }

    /// List nodes using the generic query params surface.
    pub async fn list_page(
        &self,
        params: &NodeListParams,
    ) -> Result<NodeListResponse, ClientError> {
        let mut query = Vec::new();
        if let Some(filter) = &params.filter {
            query.push(("filter", filter.clone()));
        }
        if let Some(sort) = &params.sort {
            query.push(("sort", sort.clone()));
        }
        if let Some(page) = params.page {
            query.push(("page", page.to_string()));
        }
        if let Some(size) = params.size {
            query.push(("size", size.to_string()));
        }
        self.http
            .get_query::<NodeListResponse>(&format!("{}/nodes", self.base), &query)
            .await
    }

    /// Get a single node by its canonical path (e.g. `/station/floor1/ahu-5`).
    pub async fn get(&self, path: &str) -> Result<NodeSnapshot, ClientError> {
        let encoded = urlencoding_path(path);
        self.http
            .get::<NodeSnapshot>(&format!("{}/node?path={encoded}", self.base))
            .await
    }

    /// Get the kind-declared slot schemas for one node. Lets a client
    /// answer "what slots does this node have, and what shape does
    /// each carry?" without cross-referencing the full kind registry.
    ///
    /// Internal bookkeeping slots (marked `is_internal` in the
    /// manifest) are filtered out by default — pass `include_internal:
    /// true` to see them.
    ///
    /// Example: `client.nodes().schema("/flow-1/heartbeat", false).await?`.
    pub async fn schema(
        &self,
        path: &str,
        include_internal: bool,
    ) -> Result<NodeSchema, ClientError> {
        let encoded = urlencoding_path(path);
        let url = format!(
            "{}/node/schema?path={encoded}&include_internal={include_internal}",
            self.base
        );
        self.http.get::<NodeSchema>(&url).await
    }

    /// Create a child node under `parent` with the given kind and name.
    pub async fn create(
        &self,
        parent: &str,
        kind: &str,
        name: &str,
    ) -> Result<CreatedNode, ClientError> {
        self.http
            .post(
                &format!("{}/nodes", self.base),
                &CreateNodeReq { parent, kind, name },
            )
            .await
    }

    /// Delete a node by path. Cascading behaviour depends on the node's kind.
    pub async fn delete(&self, path: &str) -> Result<(), ClientError> {
        let encoded = urlencoding_path(path);
        self.http
            .delete(&format!("{}/node?path={encoded}", self.base))
            .await
    }
}

/// Percent-encode only the characters that matter for a query-string
/// value. Node paths are `/a/b/c` — slashes are safe in query values
/// but spaces and special characters need encoding.
fn urlencoding_path(s: &str) -> String {
    // Minimal encoding: spaces and a few others.
    s.replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}
