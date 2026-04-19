//! Health check — `GET /healthz`.

use crate::error::ClientError;
use crate::http::HttpClient;

pub struct Health<'c> {
    http: &'c HttpClient,
}

impl<'c> Health<'c> {
    pub(crate) fn new(http: &'c HttpClient) -> Self {
        Self { http }
    }

    /// Returns `true` if the agent is reachable and healthy.
    pub async fn check(&self) -> Result<bool, ClientError> {
        let body: String = self.http.get("/healthz").await?;
        Ok(body == "ok")
    }
}
