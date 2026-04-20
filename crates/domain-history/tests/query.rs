//! Integration tests for `domain_history::query` — bucketed history,
//! bucketed telemetry, grouped-by-kind fan-out. Lives in `tests/` to
//! keep `src/query.rs` under the 400-line cap from CODE-LAYOUT.md.

use data_repos::{HistoryQuery, HistoryRecord, HistoryRepo, HistorySlotKind, RepoError};
use data_tsdb::{ScalarQuery, ScalarRecord, TelemetryRepo, TsdbError};
use domain_history::{
    bucketed_history, bucketed_telemetry, grouped_telemetry, HistoryBucketedResult, QueryError,
};
use graph::{seed, GraphStore, KindRegistry, NullSink};
use spi::{KindId, NodePath};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ---- fakes ---------------------------------------------------------------

struct FakeHistory {
    rows: Mutex<Vec<HistoryRecord>>,
}
impl HistoryRepo for FakeHistory {
    fn insert_batch(&self, _: &[HistoryRecord], _: u64) -> Result<(), RepoError> {
        Ok(())
    }
    fn query_range(&self, q: &HistoryQuery) -> Result<Vec<HistoryRecord>, RepoError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| {
                r.node_id == q.node_id
                    && r.slot_name == q.slot_name
                    && r.ts_ms >= q.from_ms
                    && r.ts_ms <= q.to_ms
            })
            .cloned()
            .collect())
    }
    fn count(&self, _: Uuid, _: &str) -> Result<u64, RepoError> {
        Ok(0)
    }
    fn evict_oldest(&self, _: Uuid, _: &str, _: u64) -> Result<(), RepoError> {
        Ok(())
    }
    fn bytes_in_window(&self, _: Uuid, _: &str, _: i64) -> Result<i64, RepoError> {
        Ok(0)
    }
}

struct FakeTsdb {
    rows: Mutex<Vec<ScalarRecord>>,
}
impl TelemetryRepo for FakeTsdb {
    fn insert_batch(&self, _: &[ScalarRecord], _: u64) -> Result<(), TsdbError> {
        Ok(())
    }
    fn query_range(&self, q: &ScalarQuery) -> Result<Vec<ScalarRecord>, TsdbError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| {
                r.node_id == q.node_id
                    && r.slot_name == q.slot_name
                    && r.ts_ms >= q.from_ms
                    && r.ts_ms <= q.to_ms
            })
            .cloned()
            .collect())
    }
    fn count(&self, _: Uuid, _: &str) -> Result<u64, TsdbError> {
        Ok(0)
    }
    fn evict_oldest(&self, _: Uuid, _: &str, _: u64) -> Result<(), TsdbError> {
        Ok(())
    }
}

// ---- builders ------------------------------------------------------------

fn hist_rec(ts: i64, value: &str) -> HistoryRecord {
    HistoryRecord {
        id: 0,
        node_id: Uuid::nil(),
        slot_name: "notes".into(),
        slot_kind: HistorySlotKind::String,
        ts_ms: ts,
        value_json: Some(serde_json::to_string(value).unwrap()),
        blob_bytes: None,
        byte_size: 1,
        ntp_synced: true,
        last_sync_age_ms: None,
    }
}

fn scalar_rec(nid: Uuid, ts: i64, v: f64) -> ScalarRecord {
    ScalarRecord {
        node_id: nid,
        slot_name: "value".into(),
        ts_ms: ts,
        bool_value: None,
        num_value: Some(v),
        ntp_synced: true,
        last_sync_age_ms: None,
    }
}

fn build_graph_with_two_points() -> (Arc<GraphStore>, Uuid, Uuid) {
    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
    graph.create_root(KindId::new("sys.core.station")).unwrap();
    graph
        .create_child(&NodePath::root(), KindId::new("sys.driver.demo"), "proto")
        .unwrap();
    graph
        .create_child(
            &NodePath::root().child("proto"),
            KindId::new("sys.driver.demo.device"),
            "dev",
        )
        .unwrap();
    let a = graph
        .create_child(
            &NodePath::root().child("proto").child("dev"),
            KindId::new("sys.driver.demo.point"),
            "pt_a",
        )
        .unwrap();
    let b = graph
        .create_child(
            &NodePath::root().child("proto").child("dev"),
            KindId::new("sys.driver.demo.point"),
            "pt_b",
        )
        .unwrap();
    (graph, a.0, b.0)
}

// ---- bucketed_history ---------------------------------------------------

#[test]
fn bucketed_history_last_picks_newest_in_bucket() {
    let repo = FakeHistory {
        rows: Mutex::new(vec![hist_rec(1_000, "a"), hist_rec(5_000, "b"), hist_rec(11_000, "c")]),
    };
    let r: HistoryBucketedResult = bucketed_history(
        &repo,
        Uuid::nil(),
        "notes".into(),
        0,
        20_000,
        10_000,
        Some("last"),
        None,
    )
    .unwrap();
    assert_eq!(r.rows.len(), 2);
    assert_eq!(r.rows[0].value_json.as_deref(), Some("\"b\""));
    assert_eq!(r.rows[1].value_json.as_deref(), Some("\"c\""));
}

#[test]
fn bucketed_history_rejects_unknown_agg() {
    let repo = FakeHistory { rows: Mutex::new(vec![]) };
    let err = bucketed_history(
        &repo,
        Uuid::nil(),
        "notes".into(),
        0,
        1_000,
        100,
        Some("avg"),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, QueryError::Invalid(_)));
}

#[test]
fn bucketed_history_flags_partial_edges() {
    let repo = FakeHistory {
        rows: Mutex::new(vec![hist_rec(7_000, "x"), hist_rec(17_000, "y")]),
    };
    // from=5000: first bucket ts=0 < 5000 → partial_start.
    // to=20_000: last bucket ts=10_000, ends 20_000 → not partial_end.
    let r = bucketed_history(
        &repo,
        Uuid::nil(),
        "notes".into(),
        5_000,
        20_000,
        10_000,
        Some("last"),
        None,
    )
    .unwrap();
    assert!(r.edge_partial_start);
    assert!(!r.edge_partial_end);
}

// ---- bucketed_telemetry --------------------------------------------------

#[test]
fn bucketed_telemetry_averages_samples_per_bucket() {
    let repo = FakeTsdb {
        rows: Mutex::new(vec![
            scalar_rec(Uuid::nil(), 1_000, 10.0),
            scalar_rec(Uuid::nil(), 5_000, 20.0),
            scalar_rec(Uuid::nil(), 11_000, 40.0),
            scalar_rec(Uuid::nil(), 15_000, 60.0),
        ]),
    };
    let r = bucketed_telemetry(
        &repo,
        Uuid::nil(),
        "value".into(),
        0,
        20_000,
        10_000,
        Some("avg"),
        None,
    )
    .unwrap();
    assert_eq!(r.rows.len(), 2);
    assert!((r.rows[0].value.unwrap() - 15.0).abs() < 1e-9);
    assert!((r.rows[1].value.unwrap() - 50.0).abs() < 1e-9);
}

// ---- grouped_telemetry ---------------------------------------------------

#[test]
fn grouped_telemetry_fans_out_over_kind() {
    let (graph, a, b) = build_graph_with_two_points();
    let repo = FakeTsdb {
        rows: Mutex::new(vec![
            scalar_rec(a, 1_000, 10.0),
            scalar_rec(a, 5_000, 20.0),
            scalar_rec(b, 3_000, 100.0),
            scalar_rec(b, 7_000, 200.0),
        ]),
    };
    let r = grouped_telemetry(
        &repo,
        &graph,
        "sys.driver.demo.point",
        "value".into(),
        0,
        10_000,
        Some(10_000),
        Some("avg"),
        None,
    )
    .unwrap();
    assert_eq!(r.series.len(), 2);
    let mut avgs: Vec<f64> = r.series.iter().map(|s| s.rows[0].value.unwrap()).collect();
    avgs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((avgs[0] - 15.0).abs() < 1e-9);
    assert!((avgs[1] - 150.0).abs() < 1e-9);
}

#[test]
fn grouped_telemetry_requires_bucket() {
    let (graph, _, _) = build_graph_with_two_points();
    let repo = FakeTsdb { rows: Mutex::new(vec![]) };
    let err = grouped_telemetry(
        &repo,
        &graph,
        "sys.driver.demo.point",
        "value".into(),
        0,
        10_000,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, QueryError::Invalid(_)));
}

#[test]
fn grouped_telemetry_unknown_kind_returns_empty() {
    let (graph, _, _) = build_graph_with_two_points();
    let repo = FakeTsdb { rows: Mutex::new(vec![]) };
    let r = grouped_telemetry(
        &repo,
        &graph,
        "sys.nope.nope",
        "value".into(),
        0,
        10_000,
        Some(10_000),
        Some("avg"),
        None,
    )
    .unwrap();
    assert_eq!(r.series.len(), 0);
}
