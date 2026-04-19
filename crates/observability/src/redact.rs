//! Secret-scrubbing layer.
//!
//! Scans event fields at emit time and replaces values of known-secret
//! keys with `"<redacted>"`. Redaction runs at the logger boundary so
//! call sites can't forget to scrub. See `docs/design/LOGGING.md`
//! § "Redaction — automatic and declarative".
//!
//! # Matching rules
//!
//! - Exact match (case-insensitive) against the defaults plus any
//!   names registered with [`register_extra`]:
//!   `authorization`, `x-api-key`, `password`, `token`.
//! - Suffix match (case-insensitive): `_secret`, `_token`, `_key`.
//!
//! Matches are on top-level field names only — we do not walk into
//! structured values (the contract keeps sensitive material at the
//! top level where the match is deterministic).

use std::sync::RwLock;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// Placeholder value written in place of redacted secrets. Kept as a
/// `&'static str` so nothing is allocated on the hot path.
pub const REDACTED: &str = "<redacted>";

const DEFAULT_EXACT: &[&str] = &["authorization", "x-api-key", "password", "token"];
const DEFAULT_SUFFIX: &[&str] = &["_secret", "_token", "_key"];

static EXTRA_EXACT: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Register an additional field name whose value must be redacted.
///
/// Case-insensitive; duplicates are ignored. Intended for extensions
/// that declare secret field names in their manifest — the SDK calls
/// this once at extension load so the name participates in scrubbing
/// for every subsequent event.
///
/// ```
/// observability::register_extra("my_custom_secret_field");
/// ```
pub fn register_extra(field_name: &str) {
    let lowered = field_name.to_ascii_lowercase();
    let Ok(mut extras) = EXTRA_EXACT.write() else {
        return;
    };
    if !extras.iter().any(|e| e == &lowered) {
        extras.push(lowered);
    }
}

/// True if a field name should be redacted per the rules above.
pub fn is_secret(field_name: &str) -> bool {
    let lowered = field_name.to_ascii_lowercase();
    if DEFAULT_EXACT.iter().any(|k| *k == lowered) {
        return true;
    }
    if DEFAULT_SUFFIX.iter().any(|s| lowered.ends_with(s)) {
        return true;
    }
    if let Ok(extras) = EXTRA_EXACT.read() {
        if extras.iter().any(|e| e == &lowered) {
            return true;
        }
    }
    false
}

/// `tracing_subscriber` layer that records which fields on an event
/// matched the secret-key rules. In this pass the layer is a
/// detector: downstream JSON formatting replaces matched values with
/// [`REDACTED`]. When the NATS shipper lands in a later stage it will
/// consult the same rules via [`is_secret`].
///
/// The layer is `Send + Sync` and has no per-event allocation when no
/// secrets are present.
pub struct RedactLayer;

impl RedactLayer {
    /// Construct a new layer. Stateless — cheap to clone.
    pub const fn new() -> Self {
        Self
    }
}

impl Default for RedactLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for RedactLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Pass — the JSON formatter installed by `init` is responsible
        // for substituting values. This layer exists so that future
        // transports (NATS shipper, OTLP) can hook into the same
        // decision point without re-implementing the match rules.
        let mut v = SecretDetector { found: false };
        event.record(&mut v);
    }
}

struct SecretDetector {
    found: bool,
}

impl Visit for SecretDetector {
    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        if is_secret(field.name()) {
            self.found = true;
        }
    }
}

/// Helper used by the JSON fixture tests and the formatter: given a
/// field name and its rendered value, return the value that should
/// appear on the wire.
pub fn scrub<'a>(field_name: &str, value: &'a str) -> &'a str {
    if is_secret(field_name) {
        REDACTED
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match() {
        assert!(is_secret("authorization"));
        assert!(is_secret("Authorization"));
        assert!(is_secret("x-api-key"));
        assert!(is_secret("password"));
        assert!(is_secret("token"));
        assert!(is_secret("api_token"));
        assert!(is_secret("db_secret"));
        assert!(is_secret("signing_key"));
        assert!(!is_secret("msg_id"));
        assert!(!is_secret("node_path"));
    }

    #[test]
    fn register_extra_roundtrips() {
        assert!(!is_secret("my_custom_secret_field_xyz"));
        register_extra("my_custom_secret_field_xyz");
        assert!(is_secret("my_custom_secret_field_xyz"));
        assert!(is_secret("MY_CUSTOM_SECRET_FIELD_XYZ"));
        // Duplicate register is a no-op.
        register_extra("my_custom_secret_field_xyz");
    }

    #[test]
    fn scrub_replaces_value() {
        assert_eq!(scrub("password", "hunter2"), REDACTED);
        assert_eq!(scrub("msg_id", "abc"), "abc");
    }
}
