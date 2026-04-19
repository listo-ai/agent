//! Transport-layer error + HTTP mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use spi::NodeId;
use thiserror::Error;

use dashboard_nodes::ContractError;
use dashboard_runtime::{BindingError, StackError};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("node `{0}` not found")]
    NotFound(NodeId),
    #[error("page `{0}` not found")]
    PageNotFound(NodeId),
    #[error("node `{node}` is `{found}`, expected `{expected}`")]
    KindMismatch {
        node: NodeId,
        expected: String,
        found: String,
    },
    #[error("context stack: {0}")]
    Stack(#[from] StackError),
    #[error("binding `{expr}` ({widget}): {err}")]
    Binding {
        widget: NodeId,
        expr: String,
        #[source]
        err: BindingError,
    },
    #[error("parameter contract: {0}")]
    Contract(#[from] ContractError),
    #[error("limit exceeded: {what} = {value}, max = {max}")]
    LimitExceeded {
        what: &'static str,
        value: usize,
        max: usize,
    },
    #[error("malformed page node `{0}`: {1}")]
    MalformedPage(NodeId, String),
    #[error("malformed widget node `{0}`: {1}")]
    MalformedWidget(NodeId, String),
}

impl TransportError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) | Self::PageNotFound(_) => StatusCode::NOT_FOUND,
            Self::KindMismatch { .. } => StatusCode::BAD_REQUEST,
            Self::LimitExceeded { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Stack(_)
            | Self::Binding { .. }
            | Self::Contract(_)
            | Self::MalformedPage(_, _)
            | Self::MalformedWidget(_, _) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl IntoResponse for TransportError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = json!({ "error": self.to_string() });
        (status, Json(body)).into_response()
    }
}
