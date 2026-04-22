//! Pure artefact-distribution logic.
//!
//! This crate decides *what* to fetch, verifies it on arrival, and
//! manages the edge-side cache. It does **not** talk to S3, HTTP, or
//! any backend — those are in `data-artifacts-s3` /
//! `data-artifacts-local` behind Cargo features.
//!
//! Consumes the [`ArtifactStore`] trait from `spi`; every backend
//! satisfies the same contract.
//!
//! See `agent/docs/design/ARTIFACTS.md`.
//!
//! STATUS: scaffolding — module skeletons with TODOs, no logic yet.

#![forbid(unsafe_code)]

pub mod cache;
pub mod distribute;
pub mod keys;
pub mod verify;

pub use verify::{VerificationError, VerifiedArtifact};
