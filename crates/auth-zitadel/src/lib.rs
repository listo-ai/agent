//! Zitadel OIDC provider for the agent auth seam.
//!
//! Offline JWT verification against a JWKS snapshot fetched from a
//! Zitadel tenant. "Offline" is load-bearing: edges operate with
//! intermittent cloud access and must be able to verify
//! already-issued tokens without a live HTTP call. The [`JwksSource`]
//! trait abstracts *where* the JWKS comes from so tests can inject a
//! static keyset, production uses [`HttpJwksSource`], and future
//! additions (S3, on-disk, in-memory-cache) slot in without touching
//! the verifier.
//!
//! **Scope of this crate** (Phase B.1 + B.2 of
//! `docs/design/SYSTEM-BOOTSTRAP.md`):
//!
//! - [`ZitadelConfig`] + [`ZitadelProvider`] implementing
//!   [`spi::AuthProvider`]
//! - JWKS fetch + in-memory cache + optional disk cache + periodic
//!   refresh
//! - Claim → [`spi::AuthContext`] mapping (user, tenant, scopes)
//! - Optional tenant pin (single-tenant edge) vs multi-tenant cloud
//!
//! **Out of scope** (tracked as TODOs in the doc):
//!
//! - Revocation / deny-list consumption (orthogonal, additive)
//! - Cloud-side enrolment that mints a Zitadel service account —
//!   needs Zitadel admin API access and is a separate scope
//! - Token caching at the edge for resumed sessions
//!
//! # Testing without a live Zitadel
//!
//! Tests mint JWTs with [`jsonwebtoken`] against a throwaway RSA
//! keypair, construct a [`JwkSet`] from the public half, feed it to
//! [`StaticJwksSource`], and verify round-trip. No HTTP and no
//! live service dependency. See `tests/verify.rs`.

mod config;
mod deny_list;
mod error;
mod jwks;
mod provider;

pub use config::ZitadelConfig;
pub use deny_list::{boxed as boxed_deny_list, DenyList, StaticDenyList};
pub use error::{ZitadelError, ZitadelResult};
pub use jwks::{DiskCache, HttpJwksSource, JwksSource, StaticJwksSource};
pub use provider::ZitadelProvider;
