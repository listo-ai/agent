//! Block operations — `GET/POST /api/v1/blocks`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{PluginRuntimeEntry, PluginRuntimeState, PluginSummary};

pub struct Plugins<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Plugins<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// List all loaded blocks.
    pub async fn list(&self) -> Result<Vec<PluginSummary>, ClientError> {
        self.http
            .get::<Vec<PluginSummary>>(&format!("{}/blocks", self.base))
            .await
    }

    /// Get a single block by its id.
    pub async fn get(&self, id: &str) -> Result<PluginSummary, ClientError> {
        let encoded = encode_value(id);
        self.http
            .get::<PluginSummary>(&format!("{}/blocks/{encoded}", self.base))
            .await
    }

    /// Enable a block (idempotent — returns 204 No Content on success).
    pub async fn enable(&self, id: &str) -> Result<(), ClientError> {
        let encoded = encode_value(id);
        self.http
            .post_no_body(&format!("{}/blocks/{encoded}/enable", self.base))
            .await
    }

    /// Disable a block (idempotent — returns 204 No Content on success).
    pub async fn disable(&self, id: &str) -> Result<(), ClientError> {
        let encoded = encode_value(id);
        self.http
            .post_no_body(&format!("{}/blocks/{encoded}/disable", self.base))
            .await
    }

    /// Trigger a full block reload scan.
    pub async fn reload(&self) -> Result<(), ClientError> {
        self.http
            .post_no_body(&format!("{}/blocks/reload", self.base))
            .await
    }

    /// Get the current process-runtime state for one block. 404 when
    /// the block isn't a process block or the host is unavailable.
    pub async fn runtime(&self, id: &str) -> Result<PluginRuntimeState, ClientError> {
        let encoded = encode_value(id);
        self.http
            .get::<PluginRuntimeState>(&format!("{}/blocks/{encoded}/runtime", self.base))
            .await
    }

    /// Snapshot every process block's runtime state. Empty when the
    /// agent has no process blocks or no host attached.
    pub async fn runtime_all(&self) -> Result<Vec<PluginRuntimeEntry>, ClientError> {
        self.http
            .get::<Vec<PluginRuntimeEntry>>(&format!("{}/blocks/runtime", self.base))
            .await
    }
}

fn encode_value(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}
