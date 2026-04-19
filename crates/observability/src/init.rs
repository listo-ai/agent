//! One-call subscriber setup.
//!
//! [`init`] is idempotent: the first call installs the global
//! subscriber; subsequent calls return [`ObservabilityError::InitFailed`]
//! instead of panicking. Applications call this exactly once at
//! process start.
//!
//! # Example
//!
//! ```no_run
//! use observability::{init, InitRole};
//!
//! init(InitRole::Cli, "info,graph=debug").expect("init logger");
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::error::ObservabilityError;
use crate::redact::RedactLayer;

/// Deployment role mirror. Kept inline (not a re-export of
/// `config::Role`) so that the dep arrow stays
/// `observability → spi + tracing + serde + thiserror` with no
/// dependency on `config`. Mirror is exhaustive and checked by a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Dev / appliance — everything in one process.
    Standalone,
    /// Edge agent — file sink by default.
    Edge,
    /// Cloud / containerised — stdout JSON for the log aggregator.
    Cloud,
}

/// Which binary is initialising the logger. Picks the default sink
/// and formatter in the absence of explicit configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitRole {
    /// Command-line tool — pretty logs on TTY, JSON on stderr
    /// otherwise. (Pretty-on-TTY is a later refinement; today this
    /// always emits JSON to stderr.)
    Cli,
    /// Long-running agent — sink is chosen from [`Role`].
    Agent { role: Role },
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Install the global `tracing` subscriber.
///
/// `filter_env` is parsed with [`EnvFilter::try_new`] — it accepts
/// `RUST_LOG` syntax (`info,graph=debug,com.example.pg=trace`). The
/// subscriber installs:
///
/// 1. The [`RedactLayer`] secret scrubber,
/// 2. A JSON formatter (one event per line, stable field names),
/// 3. The env filter.
///
/// Returns [`ObservabilityError::InitFailed`] if a subscriber is
/// already installed (in this process) or if `tracing`'s global
/// installation fails.
pub fn init(role: InitRole, filter_env: &str) -> Result<(), ObservabilityError> {
    // Parse the filter up front so a bad string does not burn the
    // one-shot init slot — callers can fix and retry.
    let filter =
        EnvFilter::try_new(filter_env).map_err(|e| ObservabilityError::BadFilter(e.to_string()))?;

    if INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ObservabilityError::InitFailed(
            "logger already initialised in this process".into(),
        ));
    }

    let result = match pick_sink(role) {
        SinkChoice::Stdout => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(false)
                .with_span_events(FmtSpan::NONE)
                .with_writer(std::io::stdout);
            tracing_subscriber::registry()
                .with(filter)
                .with(RedactLayer::new())
                .with(fmt_layer)
                .try_init()
        }
        SinkChoice::Stderr => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(false)
                .with_span_events(FmtSpan::NONE)
                .with_writer(std::io::stderr);
            tracing_subscriber::registry()
                .with(filter)
                .with(RedactLayer::new())
                .with(fmt_layer)
                .try_init()
        }
    };

    result.map_err(|e| {
        // Roll back the initialised flag so a caller that wants to
        // retry after fixing the error can do so.
        INITIALIZED.store(false, Ordering::SeqCst);
        ObservabilityError::InitFailed(e.to_string())
    })?;

    Ok(())
}

enum SinkChoice {
    Stderr,
    Stdout,
}

fn pick_sink(role: InitRole) -> SinkChoice {
    // File sink is gated on the `file` feature; for this pass it is
    // not yet implemented (the feature flag reserves the seam). All
    // roles fall back to stderr/stdout JSON until the rotating file
    // writer lands in a later stage.
    match role {
        InitRole::Cli => SinkChoice::Stderr,
        InitRole::Agent {
            role: Role::Cloud, ..
        } => SinkChoice::Stdout,
        InitRole::Agent { .. } => SinkChoice::Stderr,
    }
}
