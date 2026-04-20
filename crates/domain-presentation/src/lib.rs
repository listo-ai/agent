#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Runtime presentation state per node: status, color, icon, message.
//!
//! Kept separate from `graph` because presentation is runtime-only
//! ephemeral state; `graph` stays focused on identity and slots.
//!
//! The wire envelope ([`spi::presentation::NodePresentationUpdate`])
//! is defined in `spi` so every crate can receive updates without
//! depending on this crate.

mod patch;

pub use patch::{apply_patch, Presentation};
pub use spi::presentation::{
    NodePresentationUpdate, NodeStatus, PresentationField, PresentationPatch,
};
