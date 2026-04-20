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

/// Aggregation to apply within each bucket in [`BucketedQuery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggKind {
    /// Arithmetic mean of numeric samples.
    Avg,
    /// Minimum numeric value in the bucket.
    Min,
    /// Maximum numeric value.
    Max,
    /// Sum of numeric values.
    Sum,
    /// Value of the sample with the largest `ts_ms`.
    Last,
    /// Row count in the bucket (returned in `value`, for boolean slots too).
    Count,
}

impl AggKind {
    /// Parse the `agg` wire param. Unknown strings → `None`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "avg" | "mean" => Some(Self::Avg),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "sum" => Some(Self::Sum),
            "last" => Some(Self::Last),
            "count" => Some(Self::Count),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
            Self::Sum => "sum",
            Self::Last => "last",
            Self::Count => "count",
        }
    }
}

/// Query parameters for a bucketed scalar fetch.
///
/// Buckets are wall-clock-aligned: a row's bucket is
/// `(ts_ms / bucket_ms) * bucket_ms`, so two charts with the same
/// `bucket_ms` produce directly comparable x-axis values. The first
/// and last returned bucket may be partial relative to
/// `[from_ms, to_ms]`.
#[derive(Debug, Clone)]
pub struct BucketedQuery {
    pub node_id: Uuid,
    pub slot_name: String,
    pub from_ms: i64,
    pub to_ms: i64,
    /// Bucket width in ms (must be > 0).
    pub bucket_ms: i64,
    pub agg: AggKind,
    /// Cap on the number of returned buckets (most recent kept).
    pub limit: Option<u32>,
}

/// One bucket's aggregated value.
#[derive(Debug, Clone)]
pub struct BucketedRow {
    /// Bucket start (wall-clock-aligned, Unix ms).
    pub ts_ms: i64,
    /// Aggregated numeric value; `None` when no numeric samples fell in
    /// the bucket (e.g. agg=Avg on a Bool-only slot).
    pub value: Option<f64>,
    /// Number of raw samples that fed this bucket.
    pub count: u64,
}

#[derive(Debug, Error)]
pub enum TsdbError {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("not found")]
    NotFound,
    #[error("invalid query: {0}")]
    Invalid(String),
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

    /// Fetch bucketed/aggregated scalar data for one slot.
    ///
    /// Default implementation falls back to `query_range` + in-process
    /// bucketing, which is fine for tests and small windows but O(rows
    /// in window). SQLite/Postgres impls should override with a
    /// `GROUP BY` query that pushes the work to the DB.
    fn query_bucketed(&self, q: &BucketedQuery) -> Result<Vec<BucketedRow>, TsdbError> {
        if q.bucket_ms <= 0 {
            return Err(TsdbError::Invalid("bucket_ms must be > 0".into()));
        }
        let raw = self.query_range(&ScalarQuery {
            node_id: q.node_id,
            slot_name: q.slot_name.clone(),
            from_ms: q.from_ms,
            to_ms: q.to_ms,
            // No per-row limit: we want every sample in the window to
            // feed the aggregation. The limit applies to output buckets.
            limit: None,
        })?;
        Ok(bucket_in_memory(&raw, q.bucket_ms, q.agg, q.limit))
    }

    /// Count rows stored for a (node, slot) pair. Used by tests and
    /// health checks to verify cap enforcement without a full range scan.
    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, TsdbError>;

    /// Evict the oldest `n` rows for (node, slot). Called by the
    /// retention sweeper; normal writes use the per-write cap instead.
    fn evict_oldest(&self, node_id: Uuid, slot_name: &str, n: u64) -> Result<(), TsdbError>;
}

/// Pure in-memory bucketing helper. Exposed for tests + the default
/// `query_bucketed` fallback.
pub fn bucket_in_memory(
    records: &[ScalarRecord],
    bucket_ms: i64,
    agg: AggKind,
    limit: Option<u32>,
) -> Vec<BucketedRow> {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Accum {
        sum: f64,
        min: f64,
        max: f64,
        any_num: bool,
        count: u64,
        last_ts: i64,
        last_val: Option<f64>,
    }

    let mut buckets: BTreeMap<i64, Accum> = BTreeMap::new();
    for r in records {
        let bucket = (r.ts_ms / bucket_ms) * bucket_ms;
        let e = buckets.entry(bucket).or_insert_with(|| Accum {
            sum: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            any_num: false,
            count: 0,
            last_ts: i64::MIN,
            last_val: None,
        });
        e.count += 1;
        let num = r.num_value.or_else(|| r.bool_value.map(|b| if b { 1.0 } else { 0.0 }));
        if let Some(v) = num {
            e.any_num = true;
            e.sum += v;
            if v < e.min {
                e.min = v;
            }
            if v > e.max {
                e.max = v;
            }
            if r.ts_ms >= e.last_ts {
                e.last_ts = r.ts_ms;
                e.last_val = Some(v);
            }
        }
    }

    let mut rows: Vec<BucketedRow> = buckets
        .into_iter()
        .map(|(ts, a)| {
            let value = match agg {
                AggKind::Count => Some(a.count as f64),
                _ if !a.any_num => None,
                AggKind::Avg => Some(a.sum / a.count as f64),
                AggKind::Sum => Some(a.sum),
                AggKind::Min => Some(a.min),
                AggKind::Max => Some(a.max),
                AggKind::Last => a.last_val,
            };
            BucketedRow {
                ts_ms: ts,
                value,
                count: a.count,
            }
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
