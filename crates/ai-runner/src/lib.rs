//! `ai-runner` — unified AI provider runner.
//!
//! Supports two transport categories:
//!
//! | Transport | Providers | Auth |
//! |-----------|-----------|------|
//! | CLI subprocess | `claude` (claude-wrapper), `codex` | binary handles auth / env key |
//! | REST HTTP | Anthropic (anthropic-ai-sdk), OpenAI (async-openai) | API key |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use ai_runner::{Registry, RunConfig, Provider};
//!
//! #[tokio::main]
//! async fn main() {
//!     let registry = Arc::new(Registry::with_defaults());
//!
//!     let runner = registry.get(&Provider::Claude).unwrap();
//!     let cfg = RunConfig { prompt: "explain Rust lifetimes".into(), ..Default::default() };
//!     let result = runner.run(cfg, "session-1".into(), Arc::new(|ev| {
//!         println!("{ev:?}");
//!     })).await;
//!
//!     println!("{}", result.text);
//! }
//! ```

pub mod defaults;
pub mod registry;
pub mod runner;
pub mod runners;
pub mod types;

pub use defaults::AiDefaults;
pub use registry::{ProviderStatus, Registry};
pub use runner::{OnEvent, Runner};
pub use types::{
    Event, EventKind, HistoryMessage, Provider, RunConfig, RunResult, ToolCallEntry, ToolChoice,
    ToolDef, ToolUse,
};
