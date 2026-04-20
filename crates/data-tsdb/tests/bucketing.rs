//! Integration tests for `TelemetryRepo::query_bucketed` on the
//! SQLite backend. Kept out of `src/sqlite.rs` so that file stays
//! under the 400-line cap from `docs/design/CODE-LAYOUT.md`.

use data_tsdb::sqlite::SqliteTelemetryRepo;
use data_tsdb::{AggKind, BucketedQuery, ScalarRecord, TelemetryRepo};
use uuid::Uuid;

fn mk() -> SqliteTelemetryRepo {
    SqliteTelemetryRepo::open_memory().unwrap()
}

fn rec(num: f64, ts: i64) -> ScalarRecord {
    ScalarRecord {
        node_id: Uuid::nil(),
        slot_name: "temp".into(),
        ts_ms: ts,
        bool_value: None,
        num_value: Some(num),
        ntp_synced: true,
        last_sync_age_ms: None,
    }
}

fn base_query() -> BucketedQuery {
    BucketedQuery {
        node_id: Uuid::nil(),
        slot_name: "temp".into(),
        from_ms: 0,
        to_ms: 20_000,
        bucket_ms: 10_000,
        agg: AggKind::Avg,
        limit: None,
    }
}

#[test]
fn avg_groups_wall_clock_buckets() {
    let repo = mk();
    // buckets of width 10_000 ms:
    //   [0, 10000): ts=1000→10, 5000→20, 9000→30   → avg 20
    //   [10000, 20000): ts=11000→40, 15000→60      → avg 50
    let batch: Vec<ScalarRecord> = [
        (1_000, 10.0),
        (5_000, 20.0),
        (9_000, 30.0),
        (11_000, 40.0),
        (15_000, 60.0),
    ]
    .iter()
    .map(|(ts, v)| rec(*v, *ts))
    .collect();
    repo.insert_batch(&batch, 100).unwrap();

    let rows = repo.query_bucketed(&base_query()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].ts_ms, 0);
    assert!((rows[0].value.unwrap() - 20.0).abs() < 1e-9);
    assert_eq!(rows[0].count, 3);
    assert_eq!(rows[1].ts_ms, 10_000);
    assert!((rows[1].value.unwrap() - 50.0).abs() < 1e-9);
    assert_eq!(rows[1].count, 2);
}

#[test]
fn min_max_sum_last_produce_expected_values() {
    let repo = mk();
    let batch: Vec<ScalarRecord> = [
        (1_000, 10.0),
        (5_000, 20.0),
        (9_000, 30.0),
        (11_000, 40.0),
        (15_000, 60.0),
    ]
    .iter()
    .map(|(ts, v)| rec(*v, *ts))
    .collect();
    repo.insert_batch(&batch, 100).unwrap();

    for (agg, want) in [
        (AggKind::Min, (10.0, 40.0)),
        (AggKind::Max, (30.0, 60.0)),
        (AggKind::Sum, (60.0, 100.0)),
        (AggKind::Last, (30.0, 60.0)),
    ] {
        let rows = repo
            .query_bucketed(&BucketedQuery {
                agg,
                ..base_query()
            })
            .unwrap();
        assert_eq!(rows.len(), 2, "{:?}", agg);
        assert!((rows[0].value.unwrap() - want.0).abs() < 1e-9, "{:?}", agg);
        assert!((rows[1].value.unwrap() - want.1).abs() < 1e-9, "{:?}", agg);
    }
}

#[test]
fn count_agg_returns_sample_count_per_bucket() {
    let repo = mk();
    let batch: Vec<ScalarRecord> = [
        (1_000, 10.0),
        (5_000, 20.0),
        (9_000, 30.0),
        (11_000, 40.0),
        (15_000, 60.0),
    ]
    .iter()
    .map(|(ts, v)| rec(*v, *ts))
    .collect();
    repo.insert_batch(&batch, 100).unwrap();
    let rows = repo
        .query_bucketed(&BucketedQuery {
            agg: AggKind::Count,
            ..base_query()
        })
        .unwrap();
    assert_eq!(rows[0].value, Some(3.0));
    assert_eq!(rows[1].value, Some(2.0));
}

#[test]
fn limit_keeps_newest_buckets() {
    let repo = mk();
    let batch: Vec<ScalarRecord> = (0..5).map(|i| rec(i as f64, i * 10_000 + 100)).collect();
    repo.insert_batch(&batch, 100).unwrap();
    let rows = repo
        .query_bucketed(&BucketedQuery {
            from_ms: 0,
            to_ms: 1_000_000,
            limit: Some(2),
            ..base_query()
        })
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].ts_ms, 30_000);
    assert_eq!(rows[1].ts_ms, 40_000);
}

#[test]
fn zero_bucket_ms_rejected() {
    let repo = mk();
    let err = repo
        .query_bucketed(&BucketedQuery {
            bucket_ms: 0,
            ..base_query()
        })
        .unwrap_err()
        .to_string();
    assert!(err.contains("bucket_ms"), "got: {err}");
}
