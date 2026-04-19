//! Config writes — `POST /api/v1/config`.

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::error::ClientError;
use crate::http::HttpClient;

#[derive(Serialize)]
struct SetConfigReq<'a> {
    path: &'a str,
    config: &'a JsonValue,
}

pub struct Config<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Config<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// Replace a node's config blob and re-fire `on_init`.
    pub async fn set(&self, path: &str, config: &JsonValue) -> Result<(), ClientError> {
        self.http
            .post_no_content(
                &format!("{}/config", self.base),
                &SetConfigReq { path, config },
            )
            .await
    }
}
