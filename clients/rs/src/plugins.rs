//! Plugin operations — `GET/POST /api/v1/plugins`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::PluginSummary;

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

    /// List all loaded plugins.
    pub async fn list(&self) -> Result<Vec<PluginSummary>, ClientError> {
        self.http
            .get::<Vec<PluginSummary>>(&format!("{}/plugins", self.base))
            .await
    }

    /// Get a single plugin by its id.
    pub async fn get(&self, id: &str) -> Result<PluginSummary, ClientError> {
        let encoded = encode_value(id);
        self.http
            .get::<PluginSummary>(&format!("{}/plugins/{encoded}", self.base))
            .await
    }

    /// Enable a plugin (idempotent — returns 204 No Content on success).
    pub async fn enable(&self, id: &str) -> Result<(), ClientError> {
        let encoded = encode_value(id);
        self.http
            .post_no_body(&format!("{}/plugins/{encoded}/enable", self.base))
            .await
    }

    /// Disable a plugin (idempotent — returns 204 No Content on success).
    pub async fn disable(&self, id: &str) -> Result<(), ClientError> {
        let encoded = encode_value(id);
        self.http
            .post_no_body(&format!("{}/plugins/{encoded}/disable", self.base))
            .await
    }

    /// Trigger a full plugin reload scan.
    pub async fn reload(&self) -> Result<(), ClientError> {
        self.http
            .post_no_body(&format!("{}/plugins/reload", self.base))
            .await
    }
}

fn encode_value(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}
