//! Repository trait *definitions*.
//!
//! Implementations live in `data-sqlite` and `data-postgres` — one per
//! backend, using each backend's native strengths. Domain crates depend
//! on these traits, never on a concrete backend.

mod error;
mod flow_repo;
mod device_repo;

pub use error::RepoError;
pub use flow_repo::{Flow, FlowQuery, FlowRepo};
pub use device_repo::{Device, DeviceQuery, DeviceRepo};
