//! Link operations — `GET /api/v1/links`, `POST /api/v1/links`,
//! `DELETE /api/v1/links/:id`.

use serde::Serialize;

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{CreatedLink, Link, LinkEndpointRef};

#[derive(Serialize)]
struct CreateLinkReq<'a> {
    source: &'a LinkEndpointRef,
    target: &'a LinkEndpointRef,
}

pub struct Links<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Links<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}/links"),
        }
    }

    /// List all links in the graph.
    pub async fn list(&self) -> Result<Vec<Link>, ClientError> {
        self.http.get::<Vec<Link>>(&self.base).await
    }

    /// Create a link. Returns the new link ID.
    pub async fn create(
        &self,
        source: &LinkEndpointRef,
        target: &LinkEndpointRef,
    ) -> Result<String, ClientError> {
        let resp: CreatedLink = self
            .http
            .post(&self.base, &CreateLinkReq { source, target })
            .await?;
        Ok(resp.id)
    }

    /// Remove a link by ID.
    pub async fn remove(&self, id: &str) -> Result<(), ClientError> {
        self.http.delete(&format!("{}/{id}", self.base)).await
    }
}
