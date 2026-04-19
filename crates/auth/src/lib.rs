//! Auth providers — concrete impls of `spi::AuthProvider`.
//!
//! See `docs/sessions/AUTH-SEAM.md`. This crate intentionally owns only
//! the *providers*; the `AuthContext` / `Scope` / trait shape lives in
//! `spi` so transports and the graph store can depend on them without
//! pulling in any provider implementation.

#[cfg(feature = "dev-null")]
mod dev_null;
#[cfg(feature = "static-token")]
mod static_token;

#[cfg(feature = "dev-null")]
pub use dev_null::DevNullProvider;
#[cfg(feature = "static-token")]
pub use static_token::{StaticTokenEntry, StaticTokenProvider};
