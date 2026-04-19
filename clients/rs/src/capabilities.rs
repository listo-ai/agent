//! Capability handshake — `GET /api/v1/capabilities`.

use semver::{Version, VersionReq};

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::CapabilityManifest;

/// A capability the client requires from the agent.
#[derive(Debug, Clone)]
pub struct CapabilityRequirement {
    pub id: String,
    pub range: VersionReq,
}

impl CapabilityRequirement {
    pub fn new(id: impl Into<String>, range: &str) -> Self {
        Self {
            id: id.into(),
            range: range.parse().expect("invalid semver range"),
        }
    }
}

pub struct Capabilities<'c> {
    http: &'c HttpClient,
}

impl<'c> Capabilities<'c> {
    pub(crate) fn new(http: &'c HttpClient) -> Self {
        Self { http }
    }

    /// Fetch the agent's capability manifest.
    pub async fn get_manifest(&self) -> Result<CapabilityManifest, ClientError> {
        self.http.get("/api/v1/capabilities").await
    }

    /// Fetch the manifest and assert every required capability is
    /// satisfied. Returns the manifest on success.
    pub async fn assert_requirements(
        &self,
        requirements: &[CapabilityRequirement],
    ) -> Result<CapabilityManifest, ClientError> {
        let manifest = self.get_manifest().await?;
        let mut missing = Vec::new();
        for req in requirements {
            let found = manifest.capabilities.iter().find(|c| c.id == req.id);
            let satisfied = found
                .and_then(|c| Version::parse(&c.version).ok())
                .map(|v| req.range.matches(&v))
                .unwrap_or(false);
            if !satisfied {
                let found_ver = found.map(|c| c.version.as_str()).unwrap_or("not present");
                missing.push(format!(
                    "{}: required {}, found {}",
                    req.id, req.range, found_ver,
                ));
            }
        }
        if !missing.is_empty() {
            return Err(ClientError::CapabilityMismatch(missing.join("; ")));
        }
        Ok(manifest)
    }
}
