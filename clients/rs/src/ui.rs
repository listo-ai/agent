//! Dashboard UI operations — `GET /api/v1/ui/nav`, `POST /api/v1/ui/resolve`,
//! `POST /api/v1/ui/action`, `GET /api/v1/ui/table`.
//!
//! See `docs/design/DASHBOARD.md` for the endpoint semantics and
//! `docs/design/NEW-API.md` for the client-parity rule this module
//! enforces.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{
    UiActionRequest, UiActionResponse, UiNavNode, UiResolveRequest, UiResolveResponse,
    UiTableParams, UiTableResponse,
};

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
            .get_query(
                &format!("{}/nav", self.base),
                &[("root", root_id.to_string())],
            )
            .await
    }

    /// Resolve a `ui.page` into a render tree + subscription metadata.
    ///
    /// Set `req.dry_run = true` to get structured validation errors
    /// without producing a render tree — used by AI authoring tools.
    pub async fn resolve(&self, req: &UiResolveRequest) -> Result<UiResolveResponse, ClientError> {
        self.http.post(&format!("{}/resolve", self.base), req).await
    }

    /// Dispatch a named action and receive a response.
    ///
    /// The `handler` field in `req` must match a handler registered on
    /// the server. An unregistered name returns
    /// [`ClientError::HttpError`] with status 404.
    ///
    /// Example:
    /// ```no_run
    /// # async fn e(c: &agent_client::AgentClient) -> Result<(), agent_client::ClientError> {
    /// use agent_client::types::{UiActionRequest, UiActionContext};
    /// let resp = c.ui().action(&UiActionRequest {
    ///     handler: "com.acme.hello.greet".into(),
    ///     args: serde_json::json!({ "name": "World" }),
    ///     context: UiActionContext::default(),
    /// }).await?;
    /// # println!("{resp:?}"); Ok(()) }
    /// ```
    pub async fn action(&self, req: &UiActionRequest) -> Result<UiActionResponse, ClientError> {
        self.http.post(&format!("{}/action", self.base), req).await
    }

    /// Fetch a paginated table of nodes matching `params.query`.
    ///
    /// The `query` field is the RSQL string from a `Table` component's
    /// `source.query`. Pagination and extra filtering via `params.filter`.
    pub async fn table(&self, params: &UiTableParams) -> Result<UiTableResponse, ClientError> {
        let mut qp: Vec<(String, String)> = Vec::new();
        if !params.query.is_empty() {
            qp.push(("query".into(), params.query.clone()));
        }
        if let Some(f) = &params.filter {
            qp.push(("filter".into(), f.clone()));
        }
        if let Some(s) = &params.sort {
            qp.push(("sort".into(), s.clone()));
        }
        if let Some(p) = params.page {
            qp.push(("page".into(), p.to_string()));
        }
        if let Some(sz) = params.size {
            qp.push(("size".into(), sz.to_string()));
        }
        if let Some(id) = &params.source_id {
            qp.push(("source_id".into(), id.clone()));
        }
        let pairs: Vec<(&str, String)> =
            qp.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        self.http
            .get_query(&format!("{}/table", self.base), &pairs)
            .await
    }
}
