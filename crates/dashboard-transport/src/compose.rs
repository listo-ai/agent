//! `POST /api/v1/ui/compose` — AI-assisted page authoring.
//!
//! Takes a natural-language prompt (plus an optional current layout
//! to edit) and returns a validated ComponentTree. The agent owns
//! the provider selection + API key via the shared `Registry`; the
//! client never sees keys. This is the canonical AI surface — the
//! CLI (`agent ui compose`) and the Studio builder's Compose panel
//! both call through here.
//!
//! The tool call's input_schema is the live `ui_ir::Component` JSON
//! Schema, so the model is physically unable to emit a component
//! variant we don't render.

use std::sync::Arc;

use ai_runner::{Provider, RunConfig, ToolChoice, ToolDef};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use ui_ir::Component;

use crate::state::DashboardState;

const MAX_TOKENS: u32 = 8000;

#[derive(Debug, Deserialize)]
pub struct ComposeRequest {
    /// Natural-language instruction from the user.
    pub prompt: String,
    /// Optional current layout to edit in place. When present the
    /// model is told to treat it as the starting point, not a rewrite.
    #[serde(default)]
    pub current_layout: Option<JsonValue>,
    /// Optional free-text hints about the surrounding graph
    /// (node paths, kinds, slots) the author wants referenced.
    #[serde(default)]
    pub context_hints: Option<String>,
    /// Override the default provider for this call (e.g. `openai`).
    #[serde(default)]
    pub provider: Option<Provider>,
    /// Override the default model for this call.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ComposeResponse {
    /// Generated ComponentTree JSON — shape-valid against emit_layout's
    /// input_schema.
    pub layout: JsonValue,
    /// Free-text the model emitted alongside the tool call, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Provider that served the request.
    pub provider: String,
}

#[derive(Debug)]
enum ComposeError {
    Unavailable(String),
    Upstream(String),
    BadRequest(String),
}

impl IntoResponse for ComposeError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "compose_unavailable", m),
            Self::Upstream(m) => (StatusCode::BAD_GATEWAY, "upstream_error", m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
        };
        (
            status,
            Json(serde_json::json!({ "code": code, "error": message })),
        )
            .into_response()
    }
}

pub async fn handler(
    State(state): State<DashboardState>,
    Json(req): Json<ComposeRequest>,
) -> Result<Json<ComposeResponse>, Response> {
    compose_inner(state, req)
        .await
        .map(Json)
        .map_err(|e| e.into_response())
}

async fn compose_inner(
    state: DashboardState,
    req: ComposeRequest,
) -> Result<ComposeResponse, ComposeError> {
    if req.prompt.trim().is_empty() {
        return Err(ComposeError::BadRequest("prompt is required".into()));
    }

    let registry = state.ai_registry.clone().ok_or_else(|| {
        ComposeError::Unavailable("AI runner not configured on the agent".into())
    })?;

    let provider = req
        .provider
        .clone()
        .or_else(|| state.ai_defaults.provider.clone())
        .unwrap_or(Provider::Anthropic);

    let runner = registry.get(&provider).ok_or_else(|| {
        ComposeError::Unavailable(format!("provider `{provider}` not registered"))
    })?;

    let component_schema = serde_json::to_value(schemars::schema_for!(Component))
        .expect("schemars component schema is infallible");

    let emit_layout = ToolDef {
        name: "emit_layout".into(),
        description: Some(
            "Emit the complete ui.page layout — always respond via this tool, never in prose."
                .into(),
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["ir_version", "root"],
            "properties": {
                "ir_version": { "type": "integer" },
                "root": component_schema,
                "vars": {
                    "type": "object",
                    "description": "Author-declared constants referenced via {{$vars.<key>}}. Use for repeated node ids, filter literals, etc."
                }
            }
        }),
    };

    let cfg = RunConfig {
        prompt: user_message(&req),
        system_prompt: Some(system_prompt()),
        model: req.model.clone().or_else(|| state.ai_defaults.model.clone()),
        api_key: state.ai_defaults.api_key_for(&provider),
        max_tokens: Some(MAX_TOKENS),
        tools: vec![emit_layout],
        tool_choice: Some(ToolChoice::Tool { name: "emit_layout".into() }),
        ..Default::default()
    };

    let session_id = format!("compose-{}", uuid_like());
    let result = runner
        .run(cfg, session_id, Arc::new(|_ev| {}))
        .await;

    if let Some(err) = result.error {
        return Err(ComposeError::Upstream(err));
    }

    let layout = result
        .tool_uses
        .into_iter()
        .find(|t| t.name == "emit_layout")
        .map(|t| t.input)
        .ok_or_else(|| {
            ComposeError::Upstream(
                "model did not call emit_layout — no layout to return".into(),
            )
        })?;

    let note_text = result.text.trim().to_string();
    let note = if note_text.is_empty() { None } else { Some(note_text) };

    Ok(ComposeResponse {
        layout,
        note,
        provider: provider.to_string(),
    })
}

fn system_prompt() -> String {
    [
        "You author live dashboards stored as `ui.page.layout` nodes.",
        "Every response must invoke the `emit_layout` tool with a complete",
        "ComponentTree. Never write raw JSON in prose. Never invent",
        "component types — only use variants present in the tool's",
        "input_schema.",
        "",
        "Binding grammar:",
        "- {{$vars.<key>}} — author-declared constants on the tree.",
        "  Use this to DRY repeated ids / strings. Declare them in the",
        "  top-level `vars` field.",
        "- {{$page.<key>}} — page-local state (filter values, selected",
        "  row, chart range). Interactive components (select, toggle,",
        "  date_range) write these.",
        "- {{$stack.<alias>}} — nav-frame lookup (rare in authored pages).",
        "- {{$user.<claim>}} — verified auth claim.",
        "- {{$self.<slot>}} — slot on the page node itself.",
        "- $target.* is reserved for kind-views (`/ui/render`), never in",
        "  authored pages.",
        "",
        "Data components:",
        "- table: source.query is RSQL over node fields (path, kind,",
        "  parent_path, lifecycle). Set subscribe: true for live rows.",
        "  columns[].field is a dot path into the slot value. Source",
        "  nodes write a Msg to their output slot, so drill into the",
        "  payload: e.g. `slots.out.payload.count`, not `slots.out`.",
        "- chart, kpi, sparkline: source.{node_id, slot}. node_id must be",
        "  a real UUID the user referenced. When unsure, declare it as",
        "  a $vars key the user will fill in rather than fabricating.",
        "",
        "Quality rules:",
        "- Every component gets a stable, short, unique id.",
        "- Prefer composed pages (kpi row + filters + table) over",
        "  monolithic ones.",
        "- Do not emit deprecated or non-existent component types.",
    ]
    .join("\n")
}

fn user_message(req: &ComposeRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(req.prompt.trim().to_string());
    if let Some(current) = &req.current_layout {
        parts.push(String::new());
        parts.push(
            "Current layout — edit this in place, don't rewrite unless the user asked:".into(),
        );
        parts.push("```json".into());
        parts.push(
            serde_json::to_string_pretty(current).unwrap_or_else(|_| "<unserialisable>".into()),
        );
        parts.push("```".into());
    }
    if let Some(hints) = &req.context_hints {
        let trimmed = hints.trim();
        if !trimmed.is_empty() {
            parts.push(String::new());
            parts.push("Context:".into());
            parts.push(trimmed.to_string());
        }
    }
    parts.join("\n")
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
