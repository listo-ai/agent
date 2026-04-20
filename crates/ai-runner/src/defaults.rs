//! Shared defaults for AI runs — provider selection, per-provider keys,
//! and model override. Held alongside a [`crate::Registry`] at the app
//! state / flow context level so call sites never read env vars directly.

use crate::types::Provider;

#[derive(Clone, Debug, Default)]
pub struct AiDefaults {
    /// Provider selected when the caller doesn't override.
    pub provider: Option<Provider>,
    /// Model override (`None` = runner's default).
    pub model: Option<String>,
    /// `ANTHROPIC_API_KEY` equivalent for the Anthropic REST runner.
    pub anthropic_api_key: Option<String>,
    /// `OPENAI_API_KEY` equivalent for the OpenAI REST runner.
    pub openai_api_key: Option<String>,
}

impl AiDefaults {
    /// API key for a given provider, `None` if the provider doesn't need
    /// one (CLI runners) or it wasn't configured.
    pub fn api_key_for(&self, provider: &Provider) -> Option<String> {
        match provider {
            Provider::Anthropic => self.anthropic_api_key.clone(),
            Provider::OpenAi => self.openai_api_key.clone(),
            Provider::Claude | Provider::Codex => None,
        }
    }
}
