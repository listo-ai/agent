//! Lifecycle transitions — `POST /api/v1/lifecycle`.

use serde::Serialize;

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::LifecycleResponse;

#[derive(Serialize)]
struct LifecycleReq<'a> {
    path: &'a str,
    to: &'a str,
}

pub struct Lifecycle<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Lifecycle<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}/lifecycle"),
        }
    }

    /// Transition a node's lifecycle. `to` is a snake_case lifecycle
    /// state (e.g. `"active"`, `"disabled"`).
    pub async fn transition(&self, path: &str, to: &str) -> Result<String, ClientError> {
        let resp: LifecycleResponse = self
            .http
            .post(&self.base, &LifecycleReq { path, to })
            .await?;
        Ok(resp.to)
    }
}
