//! Axum glue for the `spi::auth` seam.
//!
//! Three things live here:
//!
//! 1. [`AxumHeaders`] — thin newtype wrapping `http::HeaderMap` so the
//!    backend-agnostic [`spi::RequestHeaders`] trait can be driven by
//!    axum's header bag without either `spi` or `auth` depending on
//!    `http`.
//! 2. `FromRequestParts<AppState> for AuthContext` — per-handler
//!    extractor. Every mutating route declares `ctx: AuthContext` as an
//!    arg; forgetting to do so is a visible diff, not a silent
//!    middleware gap (see `docs/sessions/AUTH-SEAM.md` § "What NOT to
//!    do in this landing").
//! 3. `IntoResponse for AuthError` — structured 401 / 403 mapping so
//!    the CLI, Studio, and blocks all see the same shape.

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use spi::{AuthContext, AuthError, RequestHeaders};

use crate::state::AppState;

/// Newtype adapter so `http::HeaderMap` satisfies the trait without a
/// `http` dep leaking into `spi` / `auth`.
pub struct AxumHeaders<'a>(pub &'a HeaderMap);

impl<'a> RequestHeaders for AxumHeaders<'a> {
    fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(|v| v.to_str().ok())
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AuthErrorResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = AxumHeaders(&parts.headers);
        state
            .auth_provider
            .load()
            .resolve(&headers)
            .await
            .map_err(AuthErrorResponse)
    }
}

/// Wrapper that gives `AuthError` a `Response` shape. Kept separate so
/// `AuthError` itself stays transport-neutral.
pub struct AuthErrorResponse(pub AuthError);

impl From<AuthError> for AuthErrorResponse {
    fn from(e: AuthError) -> Self {
        Self(e)
    }
}

impl IntoResponse for AuthErrorResponse {
    fn into_response(self) -> Response {
        let (status, body) = match &self.0 {
            AuthError::MissingCredentials => (StatusCode::UNAUTHORIZED, &self.0),
            AuthError::InvalidCredentials { .. } => (StatusCode::UNAUTHORIZED, &self.0),
            AuthError::MissingScope { .. } => (StatusCode::FORBIDDEN, &self.0),
            AuthError::WrongTenant => (StatusCode::FORBIDDEN, &self.0),
            AuthError::Provider(_) => (StatusCode::INTERNAL_SERVER_ERROR, &self.0),
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn axum_headers_is_case_insensitive() {
        let mut h = HeaderMap::new();
        h.insert("Authorization", "Bearer xyz".parse().unwrap());
        let a = AxumHeaders(&h);
        assert_eq!(a.get("authorization"), Some("Bearer xyz"));
        assert_eq!(a.get("x-missing"), None);
    }

    #[test]
    fn missing_credentials_maps_to_401() {
        let resp = AuthErrorResponse(AuthError::MissingCredentials).into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn missing_scope_maps_to_403() {
        let resp = AuthErrorResponse(AuthError::MissingScope {
            required: spi::Scope::WriteSlots,
            actor: "alice".into(),
        })
        .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
