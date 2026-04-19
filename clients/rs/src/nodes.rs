//! Node operations — `GET /api/v1/nodes`, `GET /api/v1/node?path=…`,
//! `POST /api/v1/nodes`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{CreatedNode, NodeSnapshot};

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

impl<'c> Nodes<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// List every node in the graph.
    pub async fn list(&self) -> Result<Vec<NodeSnapshot>, ClientError> {
        self.http
            .get::<Vec<NodeSnapshot>>(&format!("{}/nodes", self.base))
            .await
    }

    /// Get a single node by its canonical path (e.g. `/station/floor1/ahu-5`).
    pub async fn get(&self, path: &str) -> Result<NodeSnapshot, ClientError> {
        let encoded = urlencoding_path(path);
        self.http
            .get::<NodeSnapshot>(&format!("{}/node?path={encoded}", self.base))
            .await
    }

    /// Create a child node under `parent` with the given kind and name.
    pub async fn create(
        &self,
        parent: &str,
        kind: &str,
        name: &str,
    ) -> Result<CreatedNode, ClientError> {
        self.http
            .post(&format!("{}/nodes", self.base), &CreateNodeReq { parent, kind, name })
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
