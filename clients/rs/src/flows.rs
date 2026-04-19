//! Flow document operations — create, get, list, edit, undo, redo,
//! revert, delete, and history endpoints.
//!
//! # Example
//!
//! ```rust,no_run
//! use agent_client::AgentClient;
//!
//! # async fn example() -> Result<(), agent_client::ClientError> {
//! let client = AgentClient::new("http://localhost:8080");
//!
//! // Create a flow.
//! let flow = client.flows().create("my-flow", serde_json::json!({}), "alice").await?;
//!
//! // Edit it.
//! let result = client.flows()
//!     .edit(&flow.id, flow.head_revision_id.as_deref(), serde_json::json!({"nodes":[]}), "alice", "initial nodes")
//!     .await?;
//!
//! // Undo the edit.
//! client.flows().undo(&flow.id, Some(&result.head_revision_id), "alice").await?;
//! # Ok(())
//! # }
//! ```

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{FlowDto, FlowMutationResult, FlowRevisionDto};

pub struct Flows<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Flows<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }
}

// ── Struct bodies for request payloads ───────────────────────────────────────

#[derive(Debug, Serialize)]
struct CreateBody<'a> {
    name: &'a str,
    document: &'a JsonValue,
    author: &'a str,
}

#[derive(Debug, Serialize)]
struct EditBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_head: Option<&'a str>,
    document: &'a JsonValue,
    author: &'a str,
    summary: &'a str,
}

#[derive(Debug, Serialize)]
struct UndoBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_head: Option<&'a str>,
    author: &'a str,
}

#[derive(Debug, Serialize)]
struct RedoBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_head: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_target: Option<&'a str>,
    author: &'a str,
}

#[derive(Debug, Serialize)]
struct RevertBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_head: Option<&'a str>,
    target_rev_id: &'a str,
    author: &'a str,
}

// ── Methods ───────────────────────────────────────────────────────────────────

impl<'c> Flows<'c> {
    /// List all flows, newest-first by `head_seq`.
    ///
    /// `limit` defaults to 50; `offset` defaults to 0.
    pub async fn list(&self, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<FlowDto>, ClientError> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(l) = limit {
            parts.push(format!("limit={l}"));
        }
        if let Some(o) = offset {
            parts.push(format!("offset={o}"));
        }
        let path = if parts.is_empty() {
            format!("{}/flows", self.base)
        } else {
            format!("{}/flows?{}", self.base, parts.join("&"))
        };
        self.http.get::<Vec<FlowDto>>(&path).await
    }

    /// Fetch one flow by id.
    pub async fn get(&self, id: &str) -> Result<FlowDto, ClientError> {
        self.http.get::<FlowDto>(&format!("{}/flows/{id}", self.base)).await
    }

    /// Create a new flow with an initial `create` revision.
    pub async fn create(
        &self,
        name: &str,
        document: JsonValue,
        author: &str,
    ) -> Result<FlowDto, ClientError> {
        let body = CreateBody { name, document: &document, author };
        self.http.post::<FlowDto, _>(&format!("{}/flows", self.base), &body).await
    }

    /// Delete a flow (and its entire revision history).
    ///
    /// Pass `expected_head` for optimistic-concurrency checking — `409
    /// Conflict` if the live head doesn't match. Pass `None` to skip
    /// the check (unsafe; only for admin tooling).
    pub async fn delete(
        &self,
        id: &str,
        expected_head: Option<&str>,
    ) -> Result<(), ClientError> {
        let path = match expected_head {
            Some(h) => format!("{}/flows/{id}?expected_head={h}", self.base),
            None => format!("{}/flows/{id}", self.base),
        };
        self.http.delete(&path).await
    }

    /// Append a forward edit revision.
    ///
    /// `expected_head` is the caller's view of the current head — pass
    /// `None` only for the very first edit on a freshly created flow (where
    /// no revision exists yet). Returns the new `head_revision_id`.
    pub async fn edit(
        &self,
        id: &str,
        expected_head: Option<&str>,
        document: JsonValue,
        author: &str,
        summary: &str,
    ) -> Result<FlowMutationResult, ClientError> {
        let body = EditBody { expected_head, document: &document, author, summary };
        self.http
            .post::<FlowMutationResult, _>(&format!("{}/flows/{id}/edit", self.base), &body)
            .await
    }

    /// Undo the last logical edit — appends an `undo` revision.
    ///
    /// Undo is append-only: the revision log is never mutated, only
    /// extended. Redo remains reconstructable from the log.
    pub async fn undo(
        &self,
        id: &str,
        expected_head: Option<&str>,
        author: &str,
    ) -> Result<FlowMutationResult, ClientError> {
        let body = UndoBody { expected_head, author };
        self.http
            .post::<FlowMutationResult, _>(&format!("{}/flows/{id}/undo", self.base), &body)
            .await
    }

    /// Redo the next undone edit.
    ///
    /// The redo target is derived from the revision log — no cursor is
    /// stored server-side. Pass `expected_target` to guard against the
    /// two-tab stale-cursor case (`409` if it doesn't match).
    pub async fn redo(
        &self,
        id: &str,
        expected_head: Option<&str>,
        expected_target: Option<&str>,
        author: &str,
    ) -> Result<FlowMutationResult, ClientError> {
        let body = RedoBody { expected_head, expected_target, author };
        self.http
            .post::<FlowMutationResult, _>(&format!("{}/flows/{id}/redo", self.base), &body)
            .await
    }

    /// Revert the flow to the state at `target_rev_id` (appends a
    /// `revert` revision — nothing is deleted from the log).
    pub async fn revert(
        &self,
        id: &str,
        expected_head: Option<&str>,
        target_rev_id: &str,
        author: &str,
    ) -> Result<FlowMutationResult, ClientError> {
        let body = RevertBody { expected_head, target_rev_id, author };
        self.http
            .post::<FlowMutationResult, _>(&format!("{}/flows/{id}/revert", self.base), &body)
            .await
    }

    /// List revisions for a flow, newest first.
    pub async fn list_revisions(
        &self,
        id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<FlowRevisionDto>, ClientError> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(l) = limit {
            parts.push(format!("limit={l}"));
        }
        if let Some(o) = offset {
            parts.push(format!("offset={o}"));
        }
        let path = if parts.is_empty() {
            format!("{}/flows/{id}/revisions", self.base)
        } else {
            format!("{}/flows/{id}/revisions?{}", self.base, parts.join("&"))
        };
        self.http.get::<Vec<FlowRevisionDto>>(&path).await
    }

    /// Return the materialised flow document at a specific revision.
    pub async fn document_at(
        &self,
        id: &str,
        rev_id: &str,
    ) -> Result<JsonValue, ClientError> {
        self.http
            .get::<JsonValue>(&format!("{}/flows/{id}/revisions/{rev_id}", self.base))
            .await
    }
}
