//! Dashboard UI operations — `GET /api/v1/ui/nav`, `POST /api/v1/ui/resolve`.
//!
//! See `docs/design/DASHBOARD.md` for the endpoint semantics and
//! `docs/design/NEW-API.md` for the client-parity rule this module
//! enforces.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{UiNavNode, UiResolveRequest, UiResolveResponse};

pub struct Ui<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Ui<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}/ui"),
        }
    }

    /// Fetch the `ui.nav` subtree rooted at `root_id`.
    ///
    /// Example:
    /// ```no_run
    /// # async fn e(c: &agent_client::AgentClient) -> Result<(), agent_client::ClientError> {
    /// let tree = c.ui().nav("11111111-2222-3333-4444-555555555555").await?;
    /// # println!("{}", tree.id); Ok(()) }
    /// ```
    pub async fn nav(&self, root_id: &str) -> Result<UiNavNode, ClientError> {
        self.http
            .get_query(&format!("{}/nav", self.base), &[("root", root_id.to_string())])
            .await
    }

    /// Resolve a `ui.page` into a render tree + subscription metadata.
    ///
    /// Set `req.dry_run = true` to get structured validation errors
    /// without producing a render tree — used by AI authoring tools.
    pub async fn resolve(
        &self,
        req: &UiResolveRequest,
    ) -> Result<UiResolveResponse, ClientError> {
        self.http.post(&format!("{}/resolve", self.base), req).await
    }
}
