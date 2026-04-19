//! Repository trait for structured slot history (`String`/`Json`/`Binary`
//! slots).  Scalar (`Bool`/`Number`) history lives in [`data_tsdb::TelemetryRepo`].
//!
//! The storage split is a hard contract per SLOT-STORAGE.md:
//! - `Bool` / `Number` → `TelemetryRepo` (rolling-bucket, time-series tables)
//! - `String` / `Json` / `Binary` → `HistoryRepo` (regular `slot_history` table)
//!
//! Both impls (SQLite + Postgres) expose this same synchronous surface.
//! An async wrapper belongs at the transport layer, not here.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// The string/json/binary kind tag stored on a `slot_history` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySlotKind {
    String,
    Json,
    Binary,
}

impl HistorySlotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Json => "json",
            Self::Binary => "binary",
        }
    }
}

/// A single structured history record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    /// Auto-assigned by the DB on insert; `0` when constructing for insert.
    pub id: i64,
    pub node_id: Uuid,
    pub slot_name: String,
    pub slot_kind: HistorySlotKind,
    /// Wall-clock Unix timestamp in milliseconds.
    pub ts_ms: i64,
    /// Payload for String / Json records; `None` for Binary.
    pub value_json: Option<String>,
    /// Payload for Binary records; `None` for String / Json.
    pub blob_bytes: Option<Vec<u8>>,
    /// Pre-computed byte size (used for quota enforcement).
    pub byte_size: i64,
    pub ntp_synced: bool,
    pub last_sync_age_ms: Option<i64>,
}

/// Query parameters for a structured history range fetch.
#[derive(Debug, Clone)]
pub struct HistoryQuery {
    pub node_id: Uuid,
    pub slot_name: String,
    /// Inclusive start (Unix ms).
    pub from_ms: i64,
    /// Inclusive end (Unix ms).
    pub to_ms: i64,
    pub limit: Option<u32>,
}

/// Repository for structured (`String` / `Json` / `Binary`) slot history.
///
/// Cap enforcement contract (matches design doc §"Sample caps"):
/// - `insert_batch` enforces `max_samples` (row cap) in the same transaction.
/// - `insert_batch` enforces `max_bytes_per_day` (byte cap) for Binary slots.
/// - Whichever limit hits first triggers eviction of the oldest row.
pub trait HistoryRepo: Send + Sync + 'static {
    /// Insert a batch of structured history records.
    ///
    /// `max_samples` is the effective sample cap for every slot in the
    /// batch (platform default or per-slot override, resolved by caller).
    fn insert_batch(&self, records: &[HistoryRecord], max_samples: u64) -> Result<(), RepoError>;

    /// Fetch structured history for one slot within a time range.
    fn query_range(&self, q: &HistoryQuery) -> Result<Vec<HistoryRecord>, RepoError>;

    /// Count rows stored for a (node, slot) pair.
    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, RepoError>;

    /// Evict the oldest `n` rows for (node, slot).
    fn evict_oldest(&self, node_id: Uuid, slot_name: &str, n: u64) -> Result<(), RepoError>;

    /// Sum of `byte_size` for a (node, slot) in the given day window
    /// (`day_start_ms` to `day_start_ms + 86_400_000`).
    /// Returns 0 if there are no rows in the window.
    fn bytes_in_window(
        &self,
        node_id: Uuid,
        slot_name: &str,
        day_start_ms: i64,
    ) -> Result<i64, RepoError>;
}
