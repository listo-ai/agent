//! Zitadel-provider errors.
//!
//! Verification failures map through [`From<ZitadelError> for
//! spi::AuthError`] so the transport-layer response codes come out
//! right (`401` for a bad token, `403` for a wrong-tenant stamp). We
//! keep the concrete variant on the error so logs can distinguish
//! "clock skew, retry with refreshed time" from "signature is wrong,
//! black-hole the caller".

use spi::AuthError;
use thiserror::Error;

pub type ZitadelResult<T> = Result<T, ZitadelError>;

#[derive(Debug, Error)]
pub enum ZitadelError {
    #[error("missing `Authorization: Bearer …` header")]
    MissingBearer,
    #[error("token has no `kid` header — can't pick a signing key")]
    MissingKid,
    #[error("no JWKS key matches `kid={0}`")]
    UnknownKid(String),
    #[error("JWT header parse: {0}")]
    HeaderParse(String),
    #[error("JWT signature / claim check failed: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error(
        "tenant mismatch: token carries `{token_tenant}`, provider is pinned to \
         `{pinned_tenant}`"
    )]
    TenantMismatch {
        token_tenant: String,
        pinned_tenant: String,
    },
    #[error("required claim `{0}` missing from JWT payload")]
    MissingClaim(&'static str),
    #[error("JWKS fetch from `{url}`: {source}")]
    JwksFetch {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("JWKS parse from `{url}`: {source}")]
    JwksParse {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("disk cache read `{path}`: {source}")]
    CacheRead {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("disk cache write `{path}`: {source}")]
    CacheWrite {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JWKS signal: no keys have ever loaded")]
    NoKeysLoaded,
    /// Token verified cleanly but `sub` is on the deny-list.
    /// Surfaces as [`spi::AuthError::InvalidCredentials`] on the
    /// wire — we deliberately do not leak the "revoked" distinction
    /// because that would let an attacker enumerate revoked
    /// subjects. Logs on the server side distinguish.
    #[error("subject `{subject}` is on the deny-list")]
    SubjectDenied { subject: String },
}

impl From<ZitadelError> for AuthError {
    fn from(e: ZitadelError) -> Self {
        match e {
            ZitadelError::MissingBearer => AuthError::MissingCredentials,
            ZitadelError::TenantMismatch { .. } => AuthError::WrongTenant,
            ZitadelError::MissingKid
            | ZitadelError::UnknownKid(_)
            | ZitadelError::HeaderParse(_)
            | ZitadelError::Jwt(_)
            | ZitadelError::MissingClaim(_)
            // Subject-denied is reported as InvalidCredentials on the
            // wire so revoked subjects can't be enumerated; server
            // logs retain the specific reason.
            | ZitadelError::SubjectDenied { .. } => AuthError::InvalidCredentials {
                reason: e.to_string(),
            },
            // JWKS / cache failures are server-side infra — caller
            // gets a generic 500 via AuthError::Provider rather than
            // a 401 that would misleadingly blame their credentials.
            ZitadelError::JwksFetch { .. }
            | ZitadelError::JwksParse { .. }
            | ZitadelError::CacheRead { .. }
            | ZitadelError::CacheWrite { .. }
            | ZitadelError::NoKeysLoaded => AuthError::Provider(e.to_string()),
        }
    }
}
