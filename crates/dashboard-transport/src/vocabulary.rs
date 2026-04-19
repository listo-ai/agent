//! `GET /api/v1/ui/vocabulary` — JSON Schema of the `ui_ir::Component`
//! union. Lets Monaco, Studio's palette, and LLM authors discover the
//! full component vocabulary from a single endpoint.

use axum::Json;
use serde::Serialize;
use serde_json::Value as JsonValue;
use ui_ir::{Component, IR_VERSION};

#[derive(Debug, Serialize)]
pub struct VocabularyResponse {
    pub ir_version: u32,
    pub schema: JsonValue,
}

pub async fn handler() -> Json<VocabularyResponse> {
    let schema = serde_json::to_value(schemars::schema_for!(Component))
        .expect("schema generation is infallible");
    Json(VocabularyResponse {
        ir_version: IR_VERSION,
        schema,
    })
}
