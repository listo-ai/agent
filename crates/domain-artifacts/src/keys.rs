//! Re-export of typed key constructors from `spi::artifacts::keys`.
//!
//! Centralised here so callers inside the agent workspace have a short
//! path (`domain_artifacts::keys::snapshot(...)`) and `spi` stays the
//! single source of truth for the layout.
//!
//! STATUS: scaffolding — will wire once the spi dep publishes the
//! `artifacts` module.

// TODO: pub use spi::artifacts::keys::*;
