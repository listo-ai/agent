//! `domain-history` — slot value historization for the ACME platform.
//!
//! ## What lives here
//!
//! * [`config`] — the `sys.core.history.config` node kind + settings schema.
//! * [`policy`] — resolved per-slot policy (COV / Interval / OnDemand) +
//!   COV change-detection logic per [`SlotValueKind`].
//! * [`historizer`] — the in-process service that subscribes to
//!   [`graph::GraphEvent::SlotChanged`], applies policy, buffers records,
//!   and bulk-flushes to [`data_tsdb::TelemetryRepo`] /
//!   [`data_repos::HistoryRepo`].
//!
//! ## Staging note
//!
//! Stages 1–4 of SLOT-STORAGE.md are implemented here and in the
//! lower-level `data-*` crates.  Stages 5–9 (REST query path, CLI,
//! flow nodes, edge sync, soak tests) extend this crate.

pub mod config;
pub mod historizer;
pub mod policy;
pub mod query;

pub use config::{HistoryConfig, HistoryConfigSettings, SlotPolicy};
pub use historizer::Historizer;
pub use policy::{EffectivePolicy, PolicyKind};
pub use query::{
    bucketed_history, bucketed_telemetry, grouped_telemetry,
    GroupedTelemetryResult, HistoryBucketedResult, QueryError, TelemetryBucketedResult,
    TelemetrySeries,
};

/// Register every kind manifest this crate contributes.
pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(config::manifest());
}
