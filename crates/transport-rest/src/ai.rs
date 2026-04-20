//! `/api/v1/ai/*` — direct access to the shared `ai-runner` registry.
//!
//! Two endpoints:
//!
//! - `GET  /api/v1/ai/providers` — list registered providers with their
//!   availability flag. Lets Studio / CLI pick a provider without
//!   hard-coding the list.
//! - `POST /api/v1/ai/run`       — one-shot prompt. No tool-calling,
//!   no schema enforcement. `ui/compose` remains the structured path.
//!
//! Both endpoints return `503 ai_unavailable` when the agent was
//! launched without an AI registry (e.g. tests, in-memory profile).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use ai_runner::{EventKind, Provider, RunConfig};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/ai/providers", get(list_providers))
        .route("/api/v1/ai/run", post(run_prompt))
        .route("/api/v1/ai/stream", post(stream_prompt))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatusDto {
    /// `anthropic`, `openai`, `claude`, or `codex`.
    pub provider: String,
    /// `true` if the backend is installed / reachable. CLI runners check
    /// that their binary is on `PATH`; REST runners always report `true`
    /// (key absence is caught at run time).
    pub available: bool,
}

async fn list_providers(
    State(s): State<AppState>,
) -> Result<Json<Vec<ProviderStatusDto>>, ApiError> {
    let registry = s
        .ai_registry
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "ai_unavailable"))?;
    let mut dtos: Vec<ProviderStatusDto> = registry
        .list()
        .into_iter()
        .map(|p| ProviderStatusDto {
            provider: p.provider.to_string(),
            available: p.available,
        })
        .collect();
    dtos.sort_by(|a, b| a.provider.cmp(&b.provider));
    Ok(Json(dtos))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRunRequest {
    /// The user prompt.
    pub prompt: String,
    /// Optional system prompt / instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Override the agent's default provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Override the default model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Generation cap. Runner-specific default when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Extended thinking / reasoning effort. Accepts `low` | `medium`
    /// | `high` | `off` or a raw token-budget integer as a string.
    /// Applied by runners that support it (Anthropic REST + Claude
    /// CLI); other runners ignore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRunResponse {
    pub text: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub duration_ms: u64,
}

async fn run_prompt(
    State(s): State<AppState>,
    Json(req): Json<AiRunRequest>,
) -> Result<Json<AiRunResponse>, ApiError> {
    let (runner, cfg) = prepare_run(&s, req)?;
    let session_id = format!("ai-run-{}", uuid::Uuid::new_v4());
    let result = runner
        .run(cfg, session_id, Arc::new(|_ev| {}))
        .await;

    if let Some(err) = result.error {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("upstream: {err}"),
        ));
    }

    Ok(Json(AiRunResponse {
        text: result.text,
        provider: result.provider,
        model: result.model,
        input_tokens: result.input_tokens,
        output_tokens: result.output_tokens,
        duration_ms: result.duration_ms,
    }))
}

/// SSE-streamed variant of [`run_prompt`].
///
/// Emits one SSE event per `ai_runner::EventKind` with the kind's tag
/// as the event name (`connected`, `text`, `tool_call`, `tool_use`,
/// `done`, `error`). The final frame is always a `result` event
/// carrying the aggregated `AiRunResponse` shape so clients can treat
/// the stream as "progress deltas + final summary" without a second
/// round-trip.
async fn stream_prompt(
    State(s): State<AppState>,
    Json(req): Json<AiRunRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let (runner, cfg) = prepare_run(&s, req)?;
    let session_id = format!("ai-run-{}", uuid::Uuid::new_v4());

    // Unbounded: back-pressure is on the HTTP TCP window, not this channel.
    let (tx, rx) = mpsc::unbounded_channel::<Event>();

    let tx_events = tx.clone();
    tokio::spawn(async move {
        let on_event = Arc::new(move |ev: ai_runner::Event| {
            // Re-tag: strip session_id/provider (client doesn't need them)
            // and serialize the kind directly as the wire frame. Keeps the
            // stream shape flat and symmetric with the final `result`.
            let payload = kind_to_wire(&ev.kind);
            let tag = event_tag(&ev.kind);
            if let Ok(frame) = Event::default().event(tag).json_data(&payload) {
                let _ = tx_events.send(frame);
            }
        });
        let result = runner.run(cfg, session_id, on_event).await;
        let summary = AiRunResponse {
            text: result.text.clone(),
            provider: result.provider.clone(),
            model: result.model.clone(),
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            duration_ms: result.duration_ms,
        };
        // Emit error as its own `error` event first when present, then
        // close with a `result` frame. Clients can always treat `result`
        // as the sole terminator.
        if let Some(err) = &result.error {
            if let Ok(frame) = Event::default()
                .event("error")
                .json_data(&serde_json::json!({ "type": "error", "message": err }))
            {
                let _ = tx.send(frame);
            }
        }
        let terminal = serde_json::json!({
            "type": "result",
            "text": summary.text,
            "provider": summary.provider,
            "model": summary.model,
            "input_tokens": summary.input_tokens,
            "output_tokens": summary.output_tokens,
            "duration_ms": summary.duration_ms,
        });
        if let Ok(frame) = Event::default().event("result").json_data(&terminal) {
            let _ = tx.send(frame);
        }
    });

    let stream = UnboundedReceiverStream::new(rx).map(Ok::<_, Infallible>);
    // Chain a terminal sentinel after the receiver drains so clients see
    // a clean close (axum handles the disconnect; the explicit empty tail
    // is a no-op safety net).
    let stream = stream.chain(stream::iter(Vec::<Result<Event, Infallible>>::new()));

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

fn prepare_run(
    s: &AppState,
    req: AiRunRequest,
) -> Result<(Arc<dyn ai_runner::Runner>, RunConfig), ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    let registry = s
        .ai_registry
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "ai_unavailable"))?;
    let provider = match req.provider.as_deref() {
        Some(raw) => parse_provider(raw)?,
        None => s
            .ai_defaults
            .provider
            .clone()
            .unwrap_or(Provider::Anthropic),
    };
    let runner = registry.get(&provider).ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("provider `{provider}` not registered"),
        )
    })?;
    let cfg = RunConfig {
        prompt: req.prompt,
        system_prompt: req.system_prompt,
        model: req.model.or_else(|| s.ai_defaults.model.clone()),
        api_key: s.ai_defaults.api_key_for(&provider),
        max_tokens: req.max_tokens,
        thinking_budget: req.thinking_budget,
        ..Default::default()
    };
    Ok((runner, cfg))
}

fn event_tag(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Connected { .. } => "connected",
        EventKind::Text { .. } => "text",
        EventKind::ToolCall { .. } => "tool_call",
        EventKind::ToolUse { .. } => "tool_use",
        EventKind::Done { .. } => "done",
        EventKind::Error { .. } => "error",
    }
}

/// Serialize an `EventKind` as its flat wire-form JSON, matching the
/// `AiStreamEvent` enum in the client DTOs (tagged by `type`).
fn kind_to_wire(kind: &EventKind) -> serde_json::Value {
    match kind {
        EventKind::Connected { model } => serde_json::json!({
            "type": "connected",
            "model": model,
        }),
        EventKind::Text { content } => serde_json::json!({
            "type": "text",
            "content": content,
        }),
        EventKind::ToolCall { name } => serde_json::json!({
            "type": "tool_call",
            "name": name,
        }),
        EventKind::ToolUse { id, name, input } => serde_json::json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        }),
        EventKind::Done {
            duration_ms,
            cost_usd,
            input_tokens,
            output_tokens,
        } => serde_json::json!({
            "type": "done",
            "duration_ms": duration_ms,
            "cost_usd": cost_usd,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        }),
        EventKind::Error { message } => serde_json::json!({
            "type": "error",
            "message": message,
        }),
    }
}


fn parse_provider(raw: &str) -> Result<Provider, ApiError> {
    match raw {
        "anthropic" => Ok(Provider::Anthropic),
        "openai" => Ok(Provider::OpenAi),
        "claude" => Ok(Provider::Claude),
        "codex" => Ok(Provider::Codex),
        other => Err(ApiError::bad_request(format!(
            "unknown provider `{other}` (expected anthropic, openai, claude, codex)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_provider_happy_paths() {
        assert!(matches!(parse_provider("anthropic"), Ok(Provider::Anthropic)));
        assert!(matches!(parse_provider("openai"), Ok(Provider::OpenAi)));
        assert!(matches!(parse_provider("claude"), Ok(Provider::Claude)));
        assert!(matches!(parse_provider("codex"), Ok(Provider::Codex)));
    }

    #[test]
    fn parse_provider_unknown() {
        let err = parse_provider("grok").unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
