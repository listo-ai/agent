#![allow(clippy::unwrap_used, clippy::panic)]
//! Stage 3a-4 wire-shape contract fixtures — [`GraphEvent`] half.
//!
//! Round-trips every `.json` file under
//! `/clients/contracts/fixtures/events/` through the Rust `GraphEvent`
//! enum and asserts structural equality between the original fixture
//! and the re-serialised form. One fixture per variant keeps the
//! `#[serde(tag = "event")]` discriminator honest — if someone renames
//! a variant or reshapes its payload, exactly one fixture starts
//! failing and the reviewer sees the diff.

use std::fs;
use std::path::{Path, PathBuf};

use graph::GraphEvent;
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/graph`; fixtures live at the
    // workspace level under `/clients/contracts/fixtures/events`.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/contracts/fixtures/events")
}

fn collect_fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .expect("fixtures/events directory must exist")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "expected at least one event fixture");
    out
}

#[test]
fn every_event_fixture_round_trips() {
    // The wire fixtures carry `seq` and `ts` from the transport-layer
    // `SequencedEvent` wrapper.  `GraphEvent` does not own those fields —
    // strip them before the typed round-trip so the comparison validates only
    // the domain event shape.
    for path in collect_fixtures() {
        let raw = fs::read_to_string(&path).expect("read fixture");
        let mut original: Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{}: not valid JSON: {e}", path.display()));
        // Strip transport metadata before typed round-trip.
        if let Some(obj) = original.as_object_mut() {
            obj.remove("seq");
            obj.remove("ts");
        }
        let event: GraphEvent = serde_json::from_value(original.clone())
            .unwrap_or_else(|e| panic!("{}: not a valid GraphEvent: {e}", path.display()));
        let reserialised = serde_json::to_value(&event).expect("GraphEvent always serialises");
        assert_eq!(
            reserialised,
            original,
            "{}: round-trip mismatch\n   expected: {original:#}\n   got:      {reserialised:#}",
            path.display(),
        );
    }
}

/// Every variant must have at least one fixture — the whole point of
/// fixture-locking the wire is that adding a variant forces the author
/// to think about how it serialises.
#[test]
fn every_variant_has_a_fixture() {
    let fixtures: Vec<String> = collect_fixtures()
        .into_iter()
        .map(|p| {
            let raw = fs::read_to_string(&p).unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            v["event"].as_str().unwrap().to_string()
        })
        .collect();

    for expected in [
        "node_created",
        "node_removed",
        "node_renamed",
        "slot_changed",
        "lifecycle_transition",
        "link_added",
        "link_removed",
        "link_broken",
    ] {
        assert!(
            fixtures.iter().any(|f| f == expected),
            "no fixture for GraphEvent::{expected}",
        );
    }
}
