#![allow(clippy::unwrap_used, clippy::panic)]
//! Init-time tests.
//!
//! This integration test binary owns the global tracing subscriber
//! for its lifetime. Tests are gathered into a single `#[test]` fn
//! so ordering doesn't matter.

use observability::{init, InitRole, ObservabilityError, Role};

#[test]
fn init_contract() {
    // Bad filter errors without claiming the init slot.
    let err = init(InitRole::Cli, "target=not_a_level").expect_err("bad filter must error");
    assert!(
        matches!(err, ObservabilityError::BadFilter(_)),
        "expected BadFilter, got {err:?}"
    );

    // First valid init succeeds.
    init(
        InitRole::Agent {
            role: Role::Standalone,
        },
        "info",
    )
    .expect("first init must succeed");

    // Second valid init returns InitFailed rather than panicking.
    let err = init(InitRole::Cli, "info").expect_err("second init must error");
    assert!(
        matches!(err, ObservabilityError::InitFailed(_)),
        "expected InitFailed, got {err:?}"
    );
}
