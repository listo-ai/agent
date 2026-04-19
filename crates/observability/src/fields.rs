//! Canonical log field-name constants.
//!
//! Re-export of [`spi::log`] so callers can `use observability::fields`
//! without reaching into `spi`. Single source of truth — the strings
//! are defined once in `spi::log` and reused everywhere.

pub use spi::log::{
    AGENT_ID, ALL, FLOW_ID, KIND_ID, LEVEL, LOG_SCHEMA_VERSION, LOG_SCHEMA_VERSION_FIELD, MSG,
    MSG_ID, NODE_PATH, PARENT_MSG_ID, PLUGIN_ID, PLUGIN_VERSION, REQUEST_ID, SPAN_ID, TARGET,
    TENANT_ID, TRACE_ID, TS, USER_ID,
};
