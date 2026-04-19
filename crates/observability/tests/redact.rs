#![allow(clippy::unwrap_used, clippy::panic)]
//! Redaction rule tests.
//!
//! Case sensitivity: the match is **case-insensitive** on the
//! top-level key name. Values are compared byte-for-byte — they are
//! never inspected. Nested structure is out of scope: the platform
//! keeps sensitive material at the top level where the match is
//! deterministic.

use observability::redact::{is_secret, register_extra, scrub, REDACTED};

#[test]
fn default_exact_keys_redact() {
    for key in ["authorization", "x-api-key", "password", "token"] {
        assert!(is_secret(key), "default key `{key}` should redact");
        assert_eq!(scrub(key, "sensitive"), REDACTED);
    }
}

#[test]
fn default_suffix_rules_redact() {
    for key in [
        "db_secret",
        "api_token",
        "signing_key",
        "oauth_refresh_token",
    ] {
        assert!(is_secret(key), "suffix key `{key}` should redact");
    }
    for key in ["msg_id", "node_path", "flow_id", "target"] {
        assert!(!is_secret(key), "non-secret `{key}` should pass through");
    }
}

#[test]
fn case_insensitive_match() {
    assert!(is_secret("Authorization"));
    assert!(is_secret("X-API-KEY"));
    assert!(is_secret("PASSWORD"));
    assert!(is_secret("DB_SECRET"));
}

#[test]
fn register_extra_extends_set() {
    let key = "observability_test_custom_secret_marker";
    assert!(!is_secret(key));
    register_extra(key);
    assert!(is_secret(key));
    // Case-insensitive after registration.
    assert!(is_secret("OBSERVABILITY_TEST_CUSTOM_SECRET_MARKER"));
    // Idempotent.
    register_extra(key);
    assert!(is_secret(key));
}

#[test]
fn scrub_leaves_non_secrets() {
    assert_eq!(scrub("msg_id", "abc-123"), "abc-123");
    assert_eq!(scrub("ctx.retry_count", "3"), "3");
}
