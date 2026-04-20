/// OpenAI cloud REST runner — backed by the `async-openai` crate.
///
/// Auth: `OPENAI_API_KEY` env var, or supply via [`RunConfig::api_key`].
/// Also works with any OpenAI-compatible provider via [`RunConfig::base_url`].
use std::time::Instant;

use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use tracing::warn;

use crate::runner::{OnEvent, Runner};
use crate::types::{Event, EventKind, Provider, RunConfig, RunResult};

const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_MAX_TOKENS: u32 = 4096;

pub struct OpenAiRunner;

#[async_trait]
impl Runner for OpenAiRunner {
    fn provider(&self) -> Provider {
        Provider::OpenAi
    }

    fn available(&self) -> bool {
        true
    }

    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult {
        let mut result = RunResult {
            provider: self.provider().to_string(),
            ..Default::default()
        };

        let api_key = match cfg.api_key.clone().or_else(|| std::env::var("OPENAI_API_KEY").ok()) {
            Some(k) => k,
            None => {
                let msg = "no API key: set OPENAI_API_KEY or RunConfig::api_key".to_string();
                emit_error(&on_event, &session_id, &self.provider().to_string(), msg.clone());
                result.error = Some(msg);
                return result;
            }
        };

        let mut config = OpenAIConfig::new().with_api_key(&api_key);
        if let Some(base) = &cfg.base_url {
            config = config.with_api_base(base);
        }
        let client = Client::with_config(config);

        let model = cfg.model.as_deref().unwrap_or(DEFAULT_MODEL).to_string();
        let max_tokens = cfg.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

        // Build the messages list.
        let mut messages: Vec<async_openai::types::chat::ChatCompletionRequestMessage> = Vec::new();

        if let Some(sys) = &cfg.system_prompt {
            messages.push(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(sys.as_str())
                    .build()
                    .expect("system message build")
                    .into(),
            );
        }
        for m in &cfg.history {
            let msg: async_openai::types::chat::ChatCompletionRequestMessage = match m.role.as_str() {
                "assistant" => ChatCompletionRequestAssistantMessageArgs::default()
                    .content(m.content.as_str())
                    .build()
                    .expect("assistant message build")
                    .into(),
                _ => ChatCompletionRequestUserMessageArgs::default()
                    .content(m.content.as_str())
                    .build()
                    .expect("user message build")
                    .into(),
            };
            messages.push(msg);
        }
        messages.push(
            ChatCompletionRequestUserMessageArgs::default()
                .content(cfg.prompt.as_str())
                .build()
                .expect("user message build")
                .into(),
        );

        let request = match CreateChatCompletionRequestArgs::default()
            .model(&model)
            .max_tokens(max_tokens as u16)
            .messages(messages)
            .stream(true)
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("request build: {e}");
                emit_error(&on_event, &session_id, &self.provider().to_string(), msg.clone());
                result.error = Some(msg);
                return result;
            }
        };

        let mut stream = match client.chat().create_stream(request).await {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("request: {e}");
                emit_error(&on_event, &session_id, &self.provider().to_string(), msg.clone());
                result.error = Some(msg);
                return result;
            }
        };

        let provider_str = self.provider().to_string();
        let start = Instant::now();
        let mut text_buf = String::new();
        let mut error: Option<String> = None;
        let mut connected = false;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(response) => {
                    if !connected {
                        connected = true;
                        let m = Some(response.model.clone()).filter(|s| !s.is_empty()).or(Some(model.clone()));
                        on_event(Event {
                            session_id: session_id.clone(),
                            provider: provider_str.clone(),
                            kind: EventKind::Connected { model: m },
                        });
                    }

                    for choice in response.choices {
                        if let Some(content) = choice.delta.content {
                            if !content.is_empty() {
                                text_buf.push_str(&content);
                                on_event(Event {
                                    session_id: session_id.clone(),
                                    provider: provider_str.clone(),
                                    kind: EventKind::Text { content },
                                });
                            }
                        }
                        // finish_reason signals stream end.
                        if choice.finish_reason.is_some() {
                            on_event(Event {
                                session_id: session_id.clone(),
                                provider: provider_str.clone(),
                                kind: EventKind::Done {
                                    duration_ms: start.elapsed().as_millis() as u64,
                                    cost_usd: 0.0,
                                    input_tokens: 0,
                                    output_tokens: 0,
                                },
                            });
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("stream recv: {e}");
                    warn!(provider = "openai", "{msg}");
                    on_event(Event {
                        session_id: session_id.clone(),
                        provider: provider_str.clone(),
                        kind: EventKind::Error { message: msg.clone() },
                    });
                    error = Some(msg);
                    break;
                }
            }
        }

        result.text = text_buf;
        result.model = Some(model);
        result.duration_ms = start.elapsed().as_millis() as u64;
        result.error = error;
        result
    }
}

fn emit_error(on_event: &OnEvent, session_id: &str, provider: &str, message: String) {
    on_event(Event {
        session_id: session_id.to_string(),
        provider: provider.to_string(),
        kind: EventKind::Error { message },
    });
}
