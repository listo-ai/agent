use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::runner::Runner;
use crate::types::Provider;

/// Thread-safe registry mapping [`Provider`] → `Arc<dyn Runner>`.
///
/// Initialise with all four built-in runners via [`Registry::with_defaults`].
/// Clone the `Arc<Registry>` to share it across tasks.
#[derive(Default)]
pub struct Registry {
    runners: RwLock<HashMap<Provider, Arc<dyn Runner>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry pre-loaded with:
    /// - **CLI**: [`ClaudeRunner`] (claude-wrapper), [`CodexRunner`] (tokio process)
    /// - **REST**: [`AnthropicRunner`] (anthropic-ai-sdk), [`OpenAiRunner`] (async-openai)
    pub fn with_defaults() -> Self {
        use crate::runners::{
            anthropic::AnthropicRunner,
            claude::ClaudeRunner,
            codex::CodexRunner,
            openai::OpenAiRunner,
        };
        let r = Self::new();
        r.register(Arc::new(ClaudeRunner));
        r.register(Arc::new(CodexRunner));
        r.register(Arc::new(AnthropicRunner));
        r.register(Arc::new(OpenAiRunner));
        r
    }

    /// Register (or replace) a runner.
    pub fn register(&self, runner: Arc<dyn Runner>) {
        self.runners
            .write()
            .expect("registry lock")
            .insert(runner.provider(), runner);
    }

    /// Look up a runner by provider.
    pub fn get(&self, provider: &Provider) -> Option<Arc<dyn Runner>> {
        self.runners.read().expect("registry lock").get(provider).cloned()
    }

    /// List all registered providers with their availability.
    pub fn list(&self) -> Vec<ProviderStatus> {
        self.runners
            .read()
            .expect("registry lock")
            .values()
            .map(|r| ProviderStatus { provider: r.provider(), available: r.available() })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub available: bool,
}
