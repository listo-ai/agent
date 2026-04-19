//! Canonical log-field name contract.
//!
//! Every structured log event in the platform — core agent, CLI, Wasm
//! plugins, process plugins, Studio — uses the field names defined
//! here. They are the contract surface referenced by
//! `docs/design/LOGGING.md` § "Canonical field contract" and by
//! `docs/design/VERSIONING.md` (capability `spi.log.schema`).
//!
//! This module has no behaviour. It exists so that every producer of
//! log events references the same string constants, and a typo in a
//! field name becomes a compile error rather than a silent drift.
//!
//! The field set is frozen add-only: adding a field is a minor bump
//! of [`LOG_SCHEMA_VERSION`]; renaming or removing a field is a major
//! bump with a deprecation window.

/// Current version of the canonical log-field contract. Starts at `1`;
/// bumps only on breaking changes per `docs/design/VERSIONING.md`.
pub const LOG_SCHEMA_VERSION: u32 = 1;

// Required fields — every event includes these.

/// Event time, ISO-8601 with timezone (e.g. `2026-04-19T14:03:22.417Z`).
pub const TS: &str = "ts";
/// Severity: `trace` / `debug` / `info` / `warn` / `error`.
pub const LEVEL: &str = "level";
/// Human-readable message.
pub const MSG: &str = "msg";
/// Module path (e.g. `graph::store`, `com.example.pg.query::handler`).
pub const TARGET: &str = "target";
/// Integer schema version for this event's field contract.
pub const LOG_SCHEMA_VERSION_FIELD: &str = "log.schema_version";

// Scope-dependent fields — added automatically by the logger when the
// corresponding context is present.

/// Tenant scope for multi-tenant operations.
pub const TENANT_ID: &str = "tenant_id";
/// Authenticated user for attribution.
pub const USER_ID: &str = "user_id";
/// Agent id emitting the event (edge / standalone fleet filter).
pub const AGENT_ID: &str = "agent_id";
/// Node path for events emitted during a node-kind invocation.
pub const NODE_PATH: &str = "node_path";
/// Kind id for aggregating across instances of a kind.
pub const KIND_ID: &str = "kind_id";
/// Message id for end-to-end correlation of a `Msg` through a flow.
pub const MSG_ID: &str = "msg_id";
/// Parent message id for reconstructing fan-out / fan-in history.
pub const PARENT_MSG_ID: &str = "parent_msg_id";
/// Flow run id for filtering to a single flow execution.
pub const FLOW_ID: &str = "flow_id";
/// Request id for correlating HTTP/gRPC server + client.
pub const REQUEST_ID: &str = "request_id";
/// OpenTelemetry span id (active tracing span).
pub const SPAN_ID: &str = "span_id";
/// OpenTelemetry trace id (active tracing span).
pub const TRACE_ID: &str = "trace_id";
/// Plugin id for events emitted from an extension.
pub const PLUGIN_ID: &str = "plugin_id";
/// Plugin version for events emitted from an extension.
pub const PLUGIN_VERSION: &str = "plugin_version";

/// Every canonical field name in contract order. Useful for test
/// fixtures, schema-diff tools, and lint rules that check a producer
/// uses only known fields.
pub const ALL: &[&str] = &[
    TS,
    LEVEL,
    MSG,
    TARGET,
    LOG_SCHEMA_VERSION_FIELD,
    TENANT_ID,
    USER_ID,
    AGENT_ID,
    NODE_PATH,
    KIND_ID,
    MSG_ID,
    PARENT_MSG_ID,
    FLOW_ID,
    REQUEST_ID,
    SPAN_ID,
    TRACE_ID,
    PLUGIN_ID,
    PLUGIN_VERSION,
];
