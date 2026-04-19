#![allow(clippy::unwrap_used, clippy::panic)]
//! Stage 3a-4 wire-shape contract fixtures — [`Msg`] half.
//!
//! Round-trips every `.json` file under
//! `/clients/contracts/fixtures/msg/` through the Rust `Msg` type and
//! asserts **structural equality** (parsed JSON value, not byte-level)
//! between the original fixture and the re-serialised form. Field order
//! is intentionally not part of the contract — see `NEXT.md`.
//!
//! Stage 4's `@sys/extensions-sdk-ts` runs the same fixtures through
//! its own parser; two round-trips against the same files is how
//! "Rust SDK and TS SDK agree" stays machine-verified.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use spi::Msg;

fn fixtures_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/spi`. The fixtures live in the
    // workspace-level `/clients/contracts/fixtures/msg` directory — the
    // cross-language source of truth, see `/clients/contracts/README.md`.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/contracts/fixtures/msg")
}

fn collect_fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .expect("fixtures/msg directory must exist")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    out.sort();
    assert!(
        !out.is_empty(),
        "expected at least one msg fixture under {}",
        fixtures_dir().display(),
    );
    out
}

#[test]
fn every_msg_fixture_round_trips() {
    for path in collect_fixtures() {
        let raw = fs::read_to_string(&path).expect("read fixture");
        let original: Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{}: not valid JSON: {e}", path.display()));
        let msg: Msg = serde_json::from_value(original.clone())
            .unwrap_or_else(|e| panic!("{}: not a valid Msg: {e}", path.display()));
        let reserialised = serde_json::to_value(&msg).expect("Msg always serialises");
        assert_eq!(
            reserialised,
            original,
            "{}: round-trip mismatch\n   expected: {original:#}\n   got:      {reserialised:#}",
            path.display(),
        );
    }
}
