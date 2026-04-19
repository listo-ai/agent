//! Action-handler registry.
//!
//! Extensions register named action handlers via
//! [`HandlerRegistry::register`]. The `POST /api/v1/ui/action` endpoint
//! looks up the handler by name and dispatches to it. An unregistered
//! handler name produces a 404 — identical behaviour to resolving an
//! unknown widget type in dry-run mode.
//!
//! See `docs/design/SDUI.md` § S2 "Action dispatch".

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::RwLock;

use serde_json::Value as JsonValue;

use crate::action::{ActionContext, ActionResponse};

/// Type-erased async action handler stored in the registry.
pub type BoxActionFuture =
    Pin<Box<dyn Future<Output = Result<ActionResponse, String>> + Send + 'static>>;

pub type ActionFn =
    Box<dyn Fn(JsonValue, ActionContext) -> BoxActionFuture + Send + Sync + 'static>;

/// Registry of named action handlers contributed by extensions.
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: RwLock<HashMap<String, ActionFn>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler under `name`. If a handler with the same name
    /// already exists it is replaced (last-writer-wins, same as widget
    /// registration in extensions-host).
    pub fn register(
        &self,
        name: impl Into<String>,
        f: impl Fn(JsonValue, ActionContext) -> BoxActionFuture + Send + Sync + 'static,
    ) {
        self.handlers
            .write()
            .expect("HandlerRegistry poisoned")
            .insert(name.into(), Box::new(f));
    }

    /// Dispatch to a registered handler. Returns `None` for unregistered
    /// names (caller maps to HTTP 404).
    pub fn dispatch(
        &self,
        name: &str,
        args: JsonValue,
        ctx: ActionContext,
    ) -> Option<BoxActionFuture> {
        self.handlers
            .read()
            .expect("HandlerRegistry poisoned")
            .get(name)
            .map(|f| f(args, ctx))
    }

    /// Snapshot of registered handler names. Diagnostic / introspection.
    pub fn list(&self) -> Vec<String> {
        self.handlers
            .read()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{ActionContext, ActionResponse};

    fn make_ctx() -> ActionContext {
        ActionContext {
            target: None,
            stack: vec![],
            page_state: serde_json::Value::Null,
            auth_subject: None,
        }
    }

    #[tokio::test]
    async fn register_and_dispatch() {
        let reg = HandlerRegistry::new();
        reg.register("ping", |_args, _ctx| {
            Box::pin(async { Ok(ActionResponse::None) })
        });
        let result = reg
            .dispatch("ping", serde_json::Value::Null, make_ctx())
            .expect("handler exists")
            .await
            .unwrap();
        assert!(matches!(result, ActionResponse::None));
    }

    #[tokio::test]
    async fn unknown_handler_returns_none() {
        let reg = HandlerRegistry::new();
        assert!(reg
            .dispatch("no_such_handler", serde_json::Value::Null, make_ctx())
            .is_none());
    }

    #[tokio::test]
    async fn re_register_replaces_handler() {
        let reg = HandlerRegistry::new();
        reg.register("h", |_args, _ctx| {
            Box::pin(async {
                Ok(ActionResponse::Toast {
                    intent: crate::action::ToastIntent::Ok,
                    message: "v1".into(),
                })
            })
        });
        reg.register("h", |_args, _ctx| {
            Box::pin(async {
                Ok(ActionResponse::Toast {
                    intent: crate::action::ToastIntent::Ok,
                    message: "v2".into(),
                })
            })
        });
        let result = reg
            .dispatch("h", serde_json::Value::Null, make_ctx())
            .unwrap()
            .await
            .unwrap();
        assert!(matches!(result, ActionResponse::Toast { message, .. } if message == "v2"));
    }
}
