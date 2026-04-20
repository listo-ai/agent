//! Reusable Rust HTTP client for the agent REST API.
//!
//! This crate is a pure HTTP client — no dependency on `graph`,
//! `engine`, or any server-side crate. It mirrors the TS client at
//! `clients/ts/` in capability: same REST endpoints, same wire shapes,
//! native Rust implementation over `reqwest`.
//!
//! # Usage
//!
//! ```rust,no_run
//! use agent_client::AgentClient;
//!
//! # async fn example() -> Result<(), agent_client::ClientError> {
//! let client = AgentClient::new("http://localhost:8080");
//!
//! // List all nodes.
//! let nodes = client.nodes().list().await?;
//!
//! // Write a slot.
//! let gen = client.slots().write("/station/counter", "in", &serde_json::json!(42)).await?;
//!
//! // Check health.
//! let ok = client.health().check().await?;
//! # Ok(())
//! # }
//! ```
//!
//! The client is `Clone` and cheaply shareable (the inner `reqwest::Client`
//! uses connection pooling). Domain accessors (`nodes()`, `slots()`, …)
//! return lightweight borrowing handles — no allocation per call.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

mod ai;
mod auth;
mod blocks;
mod capabilities;
mod config;
mod error;
mod flows;
mod health;
mod http;
mod kinds;
mod lifecycle;
mod links;
mod nodes;
mod seed;
mod slots;
pub mod types;
mod ui;

pub use capabilities::CapabilityRequirement;
pub use error::ClientError;
pub use nodes::NodeListParams;

use crate::http::HttpClient;

/// REST API version this client targets.
pub const API_VERSION: u32 = 1;

/// Options for constructing an [`AgentClient`].
#[derive(Debug, Clone)]
pub struct AgentClientOptions {
    /// Base URL of the agent (e.g. `"http://localhost:8080"`).
    pub base_url: String,
    /// Optional bearer token for authenticated agents.
    pub token: Option<String>,
}

/// The public facade — entry point for all agent REST operations.
///
/// Construct via [`AgentClient::new`] (unauthenticated) or
/// [`AgentClient::with_options`]. Then call domain accessors like
/// `.nodes()`, `.slots()`, `.links()`, etc.
#[derive(Debug, Clone)]
pub struct AgentClient {
    http: HttpClient,
}

impl AgentClient {
    /// Create a client pointing at `base_url` with no auth token.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(base_url, None),
        }
    }

    /// Create a client with full options (token, etc.).
    pub fn with_options(opts: AgentClientOptions) -> Self {
        Self {
            http: HttpClient::new(opts.base_url, opts.token),
        }
    }

    /// The base URL this client was constructed with.
    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }

    // ---- domain accessors --------------------------------------------------

    pub fn nodes(&self) -> nodes::Nodes<'_> {
        nodes::Nodes::new(&self.http, API_VERSION)
    }

    pub fn slots(&self) -> slots::Slots<'_> {
        slots::Slots::new(&self.http, API_VERSION)
    }

    pub fn config(&self) -> config::Config<'_> {
        config::Config::new(&self.http, API_VERSION)
    }

    pub fn links(&self) -> links::Links<'_> {
        links::Links::new(&self.http, API_VERSION)
    }

    pub fn lifecycle(&self) -> lifecycle::Lifecycle<'_> {
        lifecycle::Lifecycle::new(&self.http, API_VERSION)
    }

    pub fn seed(&self) -> seed::Seed<'_> {
        seed::Seed::new(&self.http, API_VERSION)
    }

    pub fn capabilities(&self) -> capabilities::Capabilities<'_> {
        capabilities::Capabilities::new(&self.http)
    }

    pub fn health(&self) -> health::Health<'_> {
        health::Health::new(&self.http)
    }

    pub fn kinds(&self) -> kinds::Kinds<'_> {
        kinds::Kinds::new(&self.http, API_VERSION)
    }

    pub fn blocks(&self) -> blocks::Plugins<'_> {
        blocks::Plugins::new(&self.http, API_VERSION)
    }

    pub fn auth(&self) -> auth::Auth<'_> {
        auth::Auth::new(&self.http, API_VERSION)
    }

    pub fn ui(&self) -> ui::Ui<'_> {
        ui::Ui::new(&self.http, API_VERSION)
    }

    pub fn flows(&self) -> flows::Flows<'_> {
        flows::Flows::new(&self.http, API_VERSION)
    }

    pub fn ai(&self) -> ai::Ai<'_> {
        ai::Ai::new(&self.http, API_VERSION)
    }
}
