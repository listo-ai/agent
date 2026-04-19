//! Time-series store seam.
//!
//! [`TelemetryRepo`] is the single trait behind which the edge (SQLite
//! rolling-bucket) and cloud (Timescale hypertable) implementations
//! live.  Storage-split routing — `Bool`/`Number` → this trait,
//! `String`/`Json`/`Binary` → `HistoryRepo` — is enforced at the call
//! site in the historizer service.
//!
//! SQLite impl lives in [`sqlite`].  The Postgres/Timescale impl is a
//! Stage 4b concern; the seam is here so compilation always succeeds.

pub mod sqlite;

use thiserror::Error;
use uuid::Uuid;

/// A single scalar sample for a Bool or Number slot.
#[derive(Debug, Clone)]
pub struct ScalarRecord {
    /// Owning node UUID.
    pub node_id: Uuid,
    /// Slot name.
    pub slot_name: String,
    /// Wall-clock Unix timestamp in milliseconds.
    pub ts_ms: i64,
    /// Scalar payload — exactly one of these is `Some`.
    pub bool_value: Option<bool>,
    pub num_value: Option<f64>,
    /// Whether the edge's NTP was synced at record time.
    pub ntp_synced: bool,
    /// Milliseconds since last successful NTP sync; `None` if never synced.
    pub last_sync_age_ms: Option<i64>,
}

/// Query parameters for a scalar range fetch.
#[derive(Debug, Clone)]
pub struct ScalarQuery {
    pub node_id: Uuid,
    pub slot_name: String,
    /// Inclusive start (Unix ms).
    pub from_ms: i64,
    /// Inclusive end (Unix ms).
    pub to_ms: i64,
    /// Optional row cap on the result set.
    pub limit: Option<u32>,
}

#[derive(Debug, Error)]
pub enum TsdbError {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("not found")]
    NotFound,
}

/// Time-series repository for scalar (`Bool` / `Number`) slots.
///
/// Both implementations are synchronous; the Postgres variant wraps its
/// async client with `block_on` until the graph's sync surface is lifted
/// (a Stage 7 concern per CODE-LAYOUT.md).
pub trait TelemetryRepo: Send + Sync + 'static {
    /// Insert a batch of scalar records.  The implementation must enforce
    /// the per-slot sample cap in the same transaction: when the new row
    /// would push the slot over its cap, the oldest row is deleted first.
    fn insert_batch(&self, records: &[ScalarRecord], max_samples: u64) -> Result<(), TsdbError>;

    /// Fetch scalar records for one slot within a time range.
    fn query_range(&self, q: &ScalarQuery) -> Result<Vec<ScalarRecord>, TsdbError>;

    /// Count rows stored for a (node, slot) pair. Used by tests and
    /// health checks to verify cap enforcement without a full range scan.
    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, TsdbError>;

    /// Evict the oldest `n` rows for (node, slot). Called by the
    /// retention sweeper; normal writes use the per-write cap instead.
    fn evict_oldest(&self, node_id: Uuid, slot_name: &str, n: u64) -> Result<(), TsdbError>;
}
