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

/// Aggregation applied within each bucket for a bucketed history query.
///
/// Structured history holds JSON/String/Binary values, so the set of
/// meaningful aggregations is smaller than the scalar telemetry path:
/// `last` (newest record per bucket) and `count` (row count per bucket).
/// `avg`/`min`/`max`/`sum` don't apply to non-numeric payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAgg {
    /// The record with the greatest `ts_ms` in each bucket.
    Last,
    /// Row count in each bucket; the per-bucket value is `null`.
    Count,
}

impl HistoryAgg {
    /// Parse the `agg` wire param. Unknown strings → `None`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "last" => Some(Self::Last),
            "count" => Some(Self::Count),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Last => "last",
            Self::Count => "count",
        }
    }
}

/// Query parameters for a bucketed structured-history fetch.
///
/// Buckets are wall-clock aligned: a row's bucket is
/// `(ts_ms / bucket_ms) * bucket_ms`. See `docs/design/QUERY-LANG.md`
/// § "Time-series query shape".
#[derive(Debug, Clone)]
pub struct HistoryBucketedQuery {
    pub node_id: Uuid,
    pub slot_name: String,
    pub from_ms: i64,
    pub to_ms: i64,
    /// Bucket width in ms (must be > 0).
    pub bucket_ms: i64,
    pub agg: HistoryAgg,
    /// Cap on returned buckets (most recent kept).
    pub limit: Option<u32>,
}

/// One bucket's aggregated history value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryBucketedRow {
    /// Bucket start (wall-clock-aligned, Unix ms).
    pub ts_ms: i64,
    /// For `Last`: the JSON-encoded value of the newest record in the
    /// bucket. For `Count`: always `None`.
    pub value_json: Option<String>,
    /// Kind of the source record (mirrors `HistoryRecord::slot_kind`).
    /// For `Count` buckets with mixed kinds, reports the last record's
    /// kind for convenience. `None` when the bucket is empty.
    pub slot_kind: Option<HistorySlotKind>,
    /// Number of raw records feeding this bucket.
    pub count: u64,
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

    /// Fetch bucketed structured history.
    ///
    /// Default implementation calls `query_range` and buckets in
    /// memory — fine for the typical low-rate (event-stream) shape of
    /// structured history. A backend may override with a native SQL
    /// `GROUP BY` path if history volumes grow large enough to matter.
    fn query_bucketed(
        &self,
        q: &HistoryBucketedQuery,
    ) -> Result<Vec<HistoryBucketedRow>, RepoError> {
        if q.bucket_ms <= 0 {
            return Err(RepoError::Backend(
                "bucket_ms must be > 0".to_string(),
            ));
        }
        let raw = self.query_range(&HistoryQuery {
            node_id: q.node_id,
            slot_name: q.slot_name.clone(),
            from_ms: q.from_ms,
            to_ms: q.to_ms,
            // Bucketing needs every sample in the window; the limit
            // applies to the output bucket count, not the input rows.
            limit: None,
        })?;
        Ok(bucket_history_in_memory(&raw, q.bucket_ms, q.agg, q.limit))
    }

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

/// In-memory bucketing helper for structured history. Also used by the
/// default `HistoryRepo::query_bucketed` impl.
pub fn bucket_history_in_memory(
    records: &[HistoryRecord],
    bucket_ms: i64,
    agg: HistoryAgg,
    limit: Option<u32>,
) -> Vec<HistoryBucketedRow> {
    use std::collections::BTreeMap;

    struct Accum {
        count: u64,
        last_ts: i64,
        last_value_json: Option<String>,
        last_kind: Option<HistorySlotKind>,
    }

    let mut buckets: BTreeMap<i64, Accum> = BTreeMap::new();
    for r in records {
        let bucket = (r.ts_ms / bucket_ms) * bucket_ms;
        let e = buckets.entry(bucket).or_insert_with(|| Accum {
            count: 0,
            last_ts: i64::MIN,
            last_value_json: None,
            last_kind: None,
        });
        e.count += 1;
        if r.ts_ms >= e.last_ts {
            e.last_ts = r.ts_ms;
            e.last_value_json = r.value_json.clone();
            e.last_kind = Some(r.slot_kind);
        }
    }

    let mut rows: Vec<HistoryBucketedRow> = buckets
        .into_iter()
        .map(|(ts, a)| match agg {
            HistoryAgg::Count => HistoryBucketedRow {
                ts_ms: ts,
                value_json: None,
                slot_kind: a.last_kind,
                count: a.count,
            },
            HistoryAgg::Last => HistoryBucketedRow {
                ts_ms: ts,
                value_json: a.last_value_json,
                slot_kind: a.last_kind,
                count: a.count,
            },
        })
        .collect();

    if let Some(l) = limit {
        let l = l as usize;
        if rows.len() > l {
            let drop = rows.len() - l;
            rows.drain(0..drop);
        }
    }
    rows
}
