//! User management — `GET /api/v1/users`, `POST /api/v1/users/{id}/grants`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{GrantRoleReq, GrantRoleResp, UserDto, UserListResponse};

pub struct Users<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Users<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// List `sys.auth.user` nodes, optionally filtered.
    ///
    /// ```text
    /// client.users().list(Some("tags.labels=contains=ops"), None, None, None).await?
    /// ```
    pub async fn list(
        &self,
        filter: Option<&str>,
        sort: Option<&str>,
        page: Option<usize>,
        size: Option<usize>,
    ) -> Result<Vec<UserDto>, ClientError> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(f) = filter {
            parts.push(format!("filter={}", encode_value(f)));
        }
        if let Some(s) = sort {
            parts.push(format!("sort={}", encode_value(s)));
        }
        if let Some(p) = page {
            parts.push(format!("page={p}"));
        }
        if let Some(sz) = size {
            parts.push(format!("size={sz}"));
        }
        let path = if parts.is_empty() {
            format!("{}/users", self.base)
        } else {
            format!("{}/users?{}", self.base, parts.join("&"))
        };
        let page_resp = self
            .http
            .get::<UserListResponse>(&path)
            .await?;
        Ok(page_resp.data)
    }

    /// Grant a role to a user.
    ///
    /// ```text
    /// client.users().grant_role("user-uuid", GrantRoleReq { role: "org_admin".into(), bulk_action_id: "ba-123".into() }).await?
    /// ```
    pub async fn grant_role(
        &self,
        user_id: &str,
        req: &GrantRoleReq,
    ) -> Result<GrantRoleResp, ClientError> {
        let path = format!("{}/users/{}/grants", self.base, encode_value(user_id));
        self.http.post::<GrantRoleResp, _>(&path, req).await
    }
}

fn encode_value(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}
