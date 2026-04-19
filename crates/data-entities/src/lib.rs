//! Shared entity structs + column enums.
//!
//! Logical shape only — physical DDL diverges per backend (see
//! `data-sqlite` / `data-postgres`). Domain crates depend on these
//! types via the repo traits in `data-repos`.

pub mod ids;

pub use ids::{DeviceId, FlowId};
