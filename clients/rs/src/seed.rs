//! Seed presets — `POST /api/v1/seed`.

use serde::Serialize;

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::SeedResult;

#[derive(Serialize)]
struct SeedReq<'a> {
    preset: &'a str,
}

pub struct Seed<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Seed<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}/seed"),
        }
    }

    /// Apply a preset. Valid presets: `"count_chain"`, `"trigger_demo"`.
    pub async fn apply(&self, preset: &str) -> Result<SeedResult, ClientError> {
        self.http.post(&self.base, &SeedReq { preset }).await
    }
}
