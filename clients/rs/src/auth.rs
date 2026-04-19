//! Auth operations — `GET /api/v1/auth/whoami`.

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::WhoAmIDto;

pub struct Auth<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Auth<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// Resolve the current auth context — who the agent thinks you are,
    /// which tenant, which scopes, which provider made the call.
    ///
    /// ```rust,no_run
    /// # async fn example(client: agent_client::AgentClient) -> Result<(), agent_client::ClientError> {
    /// let who = client.auth().whoami().await?;
    /// println!("signed in as {} ({})", who.actor_display, who.provider);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn whoami(&self) -> Result<WhoAmIDto, ClientError> {
        let path = format!("{}/auth/whoami", self.base);
        self.http.get::<WhoAmIDto>(&path).await
    }
}
