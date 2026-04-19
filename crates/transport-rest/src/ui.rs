//! Minimal, vanilla-JS manual-test UI. Zero build step — served as a
//! single static HTML file so the agent has nothing to "compile" and
//! operators can open a browser on `http://localhost:8080`.
//!
//! This is **not** Studio. Studio lands in Stage 4 with React Flow,
//! Module Federation, schema-driven forms. This page is the smoke-test
//! surface: list nodes, write slot values, watch events stream in.

use axum::response::Html;

const INDEX_HTML: &str = include_str!("../static/index.html");

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}
