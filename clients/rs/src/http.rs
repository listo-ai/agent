//! Low-level HTTP helper wrapping `reqwest::Client`.
//!
//! Every domain module delegates here. The HTTP layer owns the base URL,
//! auth token, and timeout — domain modules never touch `reqwest`
//! directly.

use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::ClientError;

/// Thin wrapper around `reqwest::Client` with base-URL + auth plumbing.
#[derive(Debug, Clone)]
pub(crate) struct HttpClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl HttpClient {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            client: Client::new(),
            base_url,
            token,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let req = self.apply_auth(self.client.get(self.url(path)));
        let res = req.send().await?;
        handle_json(res).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let req = self.apply_auth(self.client.post(self.url(path)).json(body));
        let res = req.send().await?;
        handle_json(res).await
    }

    /// POST that expects 204 / empty body.
    pub async fn post_no_content<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), ClientError> {
        let req = self.apply_auth(self.client.post(self.url(path)).json(body));
        let res = req.send().await?;
        handle_empty(res).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), ClientError> {
        let req = self.apply_auth(self.client.delete(self.url(path)));
        let res = req.send().await?;
        handle_empty(res).await
    }

    /// Raw GET returning the `reqwest::Response` — used by SSE.
    #[allow(dead_code)]
    pub async fn get_raw(&self, path: &str) -> Result<Response, ClientError> {
        let req = self.apply_auth(self.client.get(self.url(path)));
        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(to_http_error(res).await);
        }
        Ok(res)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

async fn handle_json<T: DeserializeOwned>(res: Response) -> Result<T, ClientError> {
    if !res.status().is_success() {
        return Err(to_http_error(res).await);
    }
    res.json::<T>()
        .await
        .map_err(|e| ClientError::Parse(e.to_string()))
}

async fn handle_empty(res: Response) -> Result<(), ClientError> {
    if !res.status().is_success() {
        return Err(to_http_error(res).await);
    }
    Ok(())
}

async fn to_http_error(res: Response) -> ClientError {
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    ClientError::Http {
        status: status.as_u16(),
        message: if body.is_empty() {
            status.canonical_reason().unwrap_or("unknown").to_string()
        } else {
            body
        },
    }
}
