#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Repository trait *definitions*.
//!
//! Implementations live in `data-sqlite` and `data-postgres` \u{2014} one per
//! backend, using each backend's native strengths. Domain crates depend
//! on these traits, never on a concrete backend.

mod device_repo;
mod error;
mod flow_repo;
mod graph_repo;

#[cfg(feature = "testing")]
pub mod testing;

pub use device_repo::{Device, DeviceQuery, DeviceRepo};
pub use error::RepoError;
pub use flow_repo::{Flow, FlowQuery, FlowRepo};
pub use graph_repo::{GraphRepo, GraphSnapshot, PersistedLink, PersistedNode, PersistedSlot};
