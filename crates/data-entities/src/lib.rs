//! Shared entity structs + column enums.
//!
//! Logical shape only — physical DDL diverges per backend (see
//! `data-sqlite` / `data-postgres`). Domain crates depend on these
//! types via the repo traits in `data-repos`.

pub mod ids;
pub mod revisions;

pub use ids::{DeviceId, FlowId, NodeId, RevisionId};
pub use revisions::{FlowDocument, FlowRevision, RevisionOp};
