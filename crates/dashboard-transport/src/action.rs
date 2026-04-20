//! `POST /ui/action` — named action dispatch.
//!
//! Body:
//!
//! ```json
//! {
//!   "handler":    "com.acme.hello.greet",
//!   "args":       { ... },
//!   "context": {
//!     "target":       "<component-id|null>",
//!     "stack":        ["<nav-uuid>", ...],
//!     "page_state":   { ... },
//!     "auth_subject": "<subject|null>"
//!   }
//! }
//! ```
//!
//! Response is a tagged union — see [`ActionResponse`].
//!
//! An unregistered `handler` name → 404. A registered handler that
//! returns an `Err(String)` → 422 with `{ "error": "..." }`.
//!
//! See `docs/design/SDUI.md` § S2.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use ui_ir::ComponentTree;

use crate::state::DashboardState;

// ---- request ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ActionRequest {
    pub handler: String,
    #[serde(default)]
    pub args: JsonValue,
    #[serde(default)]
    pub context: ActionContext,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ActionContext {
    /// Component id that originated the action (button, form, …).
    pub target: Option<String>,
    /// Ordered nav-node ids forming the breadcrumb stack.
    #[serde(default)]
    pub stack: Vec<String>,
    /// Page-local state at the moment the action fired.
    #[serde(default)]
    pub page_state: JsonValue,
    /// Opaque auth subject identifier threaded through for audit.
    pub auth_subject: Option<String>,
}

// ---- response --------------------------------------------------------------

/// All possible action outcomes. `#[serde(tag = "type")]` produces the
/// discriminated-union shape the TS client's zod schemas expect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionResponse {
    /// Replace a single subtree in the current render tree.
    Patch {
        target_component_id: String,
        tree: ComponentTree,
    },
    /// Client-side navigation.
    Navigate { to: NavigateTo },
    /// Replace the full page render tree.
    FullRender { tree: ComponentTree },
    /// Show a transient notification.
    Toast {
        intent: ToastIntent,
        message: String,
    },
    /// Attach field-level validation errors to the originating form.
    FormErrors {
        errors: std::collections::HashMap<String, String>,
    },
    /// Trigger a file download from the given URL.
    Download { url: String },
    /// Long-running response — client subscribes to the given channel.
    Stream { channel: String },
    /// No-op — action succeeded but the UI does not need to change.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigateTo {
    pub target_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastIntent {
    Ok,
    Warn,
    Danger,
}

// ---- HTTP handler ----------------------------------------------------------

pub async fn handler(
    State(state): State<DashboardState>,
    Json(req): Json<ActionRequest>,
) -> Response {
    let fut = state.handlers.dispatch(&req.handler, req.args, req.context);

    let fut = match fut {
        Some(f) => f,
        None => {
            let body =
                serde_json::json!({ "error": format!("handler `{}` not found", req.handler) });
            return (StatusCode::NOT_FOUND, Json(body)).into_response();
        }
    };

    match fut.await {
        Ok(resp) => Json(resp).into_response(),
        Err(msg) => {
            let body = serde_json::json!({ "error": msg });
            (StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::IntoResponse as _;
    use axum::Json;
    use dashboard_runtime::InMemoryReader;

    use crate::handler_registry::HandlerRegistry;
    use crate::state::DashboardState;

    use super::*;

    fn make_state(reg: HandlerRegistry) -> DashboardState {
        DashboardState::new(Arc::new(InMemoryReader::new())).with_handlers(Arc::new(reg))
    }

    fn make_req(handler: &str) -> Json<ActionRequest> {
        Json(ActionRequest {
            handler: handler.to_string(),
            args: serde_json::Value::Null,
            context: ActionContext::default(),
        })
    }

    async fn status(resp: axum::response::Response) -> StatusCode {
        resp.status()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn unknown_handler_is_404() {
        let state = make_state(HandlerRegistry::new());
        let resp = handler(State(state), make_req("no.such.handler")).await;
        assert_eq!(status(resp).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn registered_handler_returns_toast() {
        let reg = HandlerRegistry::new();
        reg.register("test.greet", |_args, _ctx| {
            Box::pin(async {
                Ok(ActionResponse::Toast {
                    intent: ToastIntent::Ok,
                    message: "Hello!".into(),
                })
            })
        });
        let state = make_state(reg);
        let resp = handler(State(state), make_req("test.greet")).await;
        let resp = resp.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["type"], "toast");
        assert_eq!(json["intent"], "ok");
        assert_eq!(json["message"], "Hello!");
    }

    #[tokio::test]
    async fn handler_error_is_422() {
        let reg = HandlerRegistry::new();
        reg.register("test.fail", |_args, _ctx| {
            Box::pin(async { Err("something went wrong".into()) })
        });
        let state = make_state(reg);
        let resp = handler(State(state), make_req("test.fail")).await;
        assert_eq!(status(resp).await, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn none_response_is_ok() {
        let reg = HandlerRegistry::new();
        reg.register("test.noop", |_args, _ctx| {
            Box::pin(async { Ok(ActionResponse::None) })
        });
        let state = make_state(reg);
        let resp = handler(State(state), make_req("test.noop")).await;
        let resp = resp.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["type"], "none");
    }
}
