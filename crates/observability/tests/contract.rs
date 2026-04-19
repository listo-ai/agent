#![allow(clippy::unwrap_used, clippy::panic)]
//! Field-contract fixture tests.
//!
//! Parses the committed fixtures under `tests/fixtures/log/` and
//! asserts:
//!
//! 1. Every required canonical field is present with the right type.
//! 2. All top-level keys are either canonical, a `ctx.*` author
//!    field, or `_*` platform-reserved.
//! 3. `log.schema_version` equals the current
//!    [`spi::log::LOG_SCHEMA_VERSION`].
//!
//! These fixtures lock the Rust-side wire shape so a future TS mirror
//! test in Stage 4 can compare against the same bytes.

use std::fs;
use std::path::Path;

use serde_json::Value;
use spi::log as f;

const FIXTURE_DIR: &str = "tests/fixtures/log";

fn load(name: &str) -> Value {
    let path = Path::new(FIXTURE_DIR).join(name);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {path:?}: {e}"))
}

fn assert_required(value: &Value) {
    let obj = value.as_object().expect("fixture is an object");
    for key in [
        f::TS,
        f::LEVEL,
        f::MSG,
        f::TARGET,
        f::LOG_SCHEMA_VERSION_FIELD,
    ] {
        assert!(obj.contains_key(key), "missing required field: {key}");
    }
    assert!(obj[f::TS].is_string(), "ts must be a string");
    assert!(obj[f::LEVEL].is_string(), "level must be a string");
    assert!(obj[f::MSG].is_string(), "msg must be a string");
    assert!(obj[f::TARGET].is_string(), "target must be a string");
    assert_eq!(
        obj[f::LOG_SCHEMA_VERSION_FIELD].as_u64(),
        Some(u64::from(f::LOG_SCHEMA_VERSION)),
        "log.schema_version must equal the current contract version"
    );
    let level = obj[f::LEVEL].as_str().unwrap();
    assert!(
        matches!(level, "trace" | "debug" | "info" | "warn" | "error"),
        "unknown level: {level}"
    );
}

fn assert_no_unknown_toplevel(value: &Value) {
    let obj = value.as_object().expect("fixture is an object");
    for key in obj.keys() {
        let ok =
            f::ALL.iter().any(|k| *k == key) || key.starts_with("ctx.") || key.starts_with('_');
        assert!(ok, "unknown top-level key `{key}` in fixture");
    }
}

#[test]
fn minimal_event_parses() {
    let v = load("event_minimal.json");
    assert_required(&v);
    assert_no_unknown_toplevel(&v);
}

#[test]
fn full_event_parses() {
    let v = load("event_full.json");
    assert_required(&v);
    assert_no_unknown_toplevel(&v);
    let obj = v.as_object().unwrap();
    // Spot-check a representative mix of optional canonical fields.
    for k in [
        f::TENANT_ID,
        f::USER_ID,
        f::AGENT_ID,
        f::NODE_PATH,
        f::KIND_ID,
        f::MSG_ID,
        f::PARENT_MSG_ID,
        f::FLOW_ID,
        f::REQUEST_ID,
        f::SPAN_ID,
        f::TRACE_ID,
        f::PLUGIN_ID,
        f::PLUGIN_VERSION,
    ] {
        assert!(obj.contains_key(k), "full fixture missing optional `{k}`");
        assert!(obj[k].is_string() || obj[k].is_number(), "`{k}` wrong type");
    }
}

#[test]
fn redacted_event_has_placeholder_values() {
    let v = load("event_redacted.json");
    assert_required(&v);
    let obj = v.as_object().unwrap();
    for k in [
        "authorization",
        "x-api-key",
        "password",
        "token",
        "db_secret",
        "api_token",
        "signing_key",
    ] {
        assert_eq!(
            obj[k].as_str(),
            Some("<redacted>"),
            "key `{k}` should be redacted"
        );
    }
}

#[test]
fn canonical_fields_are_unique_strings() {
    // Guard against accidental duplicates in the ALL listing.
    let mut sorted = f::ALL.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        f::ALL.len(),
        "duplicate entry in spi::log::ALL"
    );
}
