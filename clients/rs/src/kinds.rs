//! Kind operations — `GET /api/v1/kinds`, `GET /api/v1/kinds/:id`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::KindDto;

pub struct Kinds<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Kinds<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// List all registered kinds, optionally filtered by facet or placeable-under constraint.
    ///
    /// `facet` is a camelCase string such as `"isContainer"` or `"isCompute"`.
    /// `placeable_under` is a kind id such as `"acme.core.station"`.
    pub async fn list(
        &self,
        facet: Option<&str>,
        placeable_under: Option<&str>,
    ) -> Result<Vec<KindDto>, ClientError> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(f) = facet {
            parts.push(format!("facet={}", encode_value(f)));
        }
        if let Some(p) = placeable_under {
            parts.push(format!("placeable_under={}", encode_value(p)));
        }
        let path = if parts.is_empty() {
            format!("{}/kinds", self.base)
        } else {
            format!("{}/kinds?{}", self.base, parts.join("&"))
        };
        self.http.get::<Vec<KindDto>>(&path).await
    }
}

fn encode_value(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}
