//! Typed query orchestration over the history + telemetry repos.
//!
//! This is the **domain** layer for the time-series query shape in
//! [`docs/design/QUERY-LANG.md`]. The orchestration — parse aggs,
//! validate buckets, compute `edge_partial_*` markers, fan out across
//! nodes-of-kind — belongs here, not in any single transport. A REST
//! handler calls these; so can the CLI, MCP tools, or an internal
//! Rust caller.
//!
//! Nothing in this file depends on Axum, HTTP, JSON, or clap. Callers
//! map [`QueryError`] to their own wire shape.

use data_repos::{
    HistoryAgg, HistoryBucketedQuery, HistoryBucketedRow, HistoryRepo,
};
use data_tsdb::{AggKind, BucketedQuery, BucketedRow, TelemetryRepo};
use graph::GraphStore;
use spi::KindId;
use thiserror::Error;
use uuid::Uuid;

/// Error variants from the query-orchestration layer. Each transport
/// maps these to its own response shape (HTTP status, CLI stderr, …).
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("invalid query: {0}")]
    Invalid(String),
    #[error("backend error: {0}")]
    Backend(String),
}

impl QueryError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

/// Result of a bucketed structured-history query.
#[derive(Debug, Clone)]
pub struct HistoryBucketedResult {
    pub rows: Vec<HistoryBucketedRow>,
    pub bucket_ms: i64,
    pub agg: HistoryAgg,
    pub from_ms: i64,
    pub to_ms: i64,
    pub edge_partial_start: bool,
    pub edge_partial_end: bool,
}

/// Result of a bucketed single-node telemetry query.
#[derive(Debug, Clone)]
pub struct TelemetryBucketedResult {
    pub rows: Vec<BucketedRow>,
    pub bucket_ms: i64,
    pub agg: AggKind,
    pub from_ms: i64,
    pub to_ms: i64,
    pub edge_partial_start: bool,
    pub edge_partial_end: bool,
}

/// One series in a [`GroupedTelemetryResult`].
#[derive(Debug, Clone)]
pub struct TelemetrySeries {
    pub node_id: Uuid,
    pub node_path: String,
    pub rows: Vec<BucketedRow>,
}

/// Result of a group-by-kind telemetry fan-out.
#[derive(Debug, Clone)]
pub struct GroupedTelemetryResult {
    pub series: Vec<TelemetrySeries>,
    pub kind: String,
    pub slot_name: String,
    pub bucket_ms: i64,
    pub agg: AggKind,
    pub from_ms: i64,
    pub to_ms: i64,
}

/// Run a bucketed structured-history query and decorate it with the
/// partial-bucket flags callers need.
///
/// Accepts `agg_str` as the raw wire string so every transport uses
/// the same parsing/error message for unknown aggs. `None` defaults to
/// `HistoryAgg::Last`.
#[allow(clippy::too_many_arguments)]
pub fn bucketed_history(
    repo: &dyn HistoryRepo,
    node_id: Uuid,
    slot_name: String,
    from_ms: i64,
    to_ms: i64,
    bucket_ms: i64,
    agg_str: Option<&str>,
    limit: Option<u32>,
) -> Result<HistoryBucketedResult, QueryError> {
    if bucket_ms <= 0 {
        return Err(QueryError::invalid("bucket must be > 0"));
    }
    let agg = match agg_str {
        None => HistoryAgg::Last,
        Some(s) => HistoryAgg::parse(s).ok_or_else(|| {
            QueryError::invalid(format!(
                "unknown agg `{s}` — history supports only `last` or `count`"
            ))
        })?,
    };

    let q = HistoryBucketedQuery {
        node_id,
        slot_name,
        from_ms,
        to_ms,
        bucket_ms,
        agg,
        limit,
    };
    let rows = repo
        .query_bucketed(&q)
        .map_err(|e| QueryError::Backend(e.to_string()))?;

    let edge_partial_start = rows.first().is_some_and(|r| r.ts_ms < from_ms);
    let edge_partial_end = rows
        .last()
        .is_some_and(|r| r.ts_ms.saturating_add(bucket_ms) > to_ms);

    Ok(HistoryBucketedResult {
        rows,
        bucket_ms,
        agg,
        from_ms,
        to_ms,
        edge_partial_start,
        edge_partial_end,
    })
}

/// Run a bucketed single-node telemetry query, decorated with
/// partial-bucket flags. Shared parsing/validation for `agg_str` +
/// `bucket_ms` so every transport behaves identically.
#[allow(clippy::too_many_arguments)]
pub fn bucketed_telemetry(
    repo: &dyn TelemetryRepo,
    node_id: Uuid,
    slot_name: String,
    from_ms: i64,
    to_ms: i64,
    bucket_ms: i64,
    agg_str: Option<&str>,
    limit: Option<u32>,
) -> Result<TelemetryBucketedResult, QueryError> {
    if bucket_ms <= 0 {
        return Err(QueryError::invalid("bucket must be > 0"));
    }
    let agg = match agg_str {
        None => AggKind::Avg,
        Some(s) => AggKind::parse(s).ok_or_else(|| {
            QueryError::invalid(format!(
                "unknown agg `{s}` — expected avg|min|max|sum|last|count"
            ))
        })?,
    };
    let rows = repo
        .query_bucketed(&BucketedQuery {
            node_id,
            slot_name,
            from_ms,
            to_ms,
            bucket_ms,
            agg,
            limit,
        })
        .map_err(|e| QueryError::Backend(e.to_string()))?;

    let edge_partial_start = rows.first().is_some_and(|r| r.ts_ms < from_ms);
    let edge_partial_end = rows
        .last()
        .is_some_and(|r| r.ts_ms.saturating_add(bucket_ms) > to_ms);

    Ok(TelemetryBucketedResult {
        rows,
        bucket_ms,
        agg,
        from_ms,
        to_ms,
        edge_partial_start,
        edge_partial_end,
    })
}

/// Run a bucketed telemetry query fanned out across every node of
/// `kind`. Requires `bucket_ms` to be set (raw rows across many nodes
/// are ambiguous without downsampling).
///
/// Node-kind is our pragmatic analog to the `group_by=tag` primitive
/// sketched in QUERY-LANG.md — we don't have a tag dimension, but we
/// do have node kinds.
#[allow(clippy::too_many_arguments)]
pub fn grouped_telemetry(
    repo: &dyn TelemetryRepo,
    graph: &GraphStore,
    kind: &str,
    slot_name: String,
    from_ms: i64,
    to_ms: i64,
    bucket_ms: Option<i64>,
    agg_str: Option<&str>,
    limit: Option<u32>,
) -> Result<GroupedTelemetryResult, QueryError> {
    let bucket_ms = bucket_ms.ok_or_else(|| {
        QueryError::invalid(
            "group-by-kind requires `bucket` — raw rows across many nodes are ambiguous",
        )
    })?;
    if bucket_ms <= 0 {
        return Err(QueryError::invalid("bucket must be > 0"));
    }
    let agg = match agg_str {
        None => AggKind::Avg,
        Some(s) => AggKind::parse(s).ok_or_else(|| {
            QueryError::invalid(format!(
                "unknown agg `{s}` — expected avg|min|max|sum|last|count"
            ))
        })?,
    };

    let target_kind = KindId::new(kind);
    let series: Vec<TelemetrySeries> = graph
        .snapshots()
        .into_iter()
        .filter(|n| n.kind == target_kind)
        .map(|node| {
            let rows = repo
                .query_bucketed(&BucketedQuery {
                    node_id: node.id.0,
                    slot_name: slot_name.clone(),
                    from_ms,
                    to_ms,
                    bucket_ms,
                    agg,
                    limit,
                })
                .map_err(|e| QueryError::Backend(e.to_string()))?;
            Ok(TelemetrySeries {
                node_id: node.id.0,
                node_path: node.path.to_string(),
                rows,
            })
        })
        .collect::<Result<_, QueryError>>()?;

    Ok(GroupedTelemetryResult {
        series,
        kind: kind.to_string(),
        slot_name,
        bucket_ms,
        agg,
        from_ms,
        to_ms,
    })
}

// Tests live in `tests/query.rs` (integration tests) to keep this
// file under the 400-line cap — see docs/design/CODE-LAYOUT.md.
