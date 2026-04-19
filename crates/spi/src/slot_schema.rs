//! Declarative slot schema — part of a kind's manifest.
//!
//! Live slot state (`SlotValue`, `SlotMap`) is graph-runtime concern and
//! stays in the `graph` crate. Only the *shape* lives here, because
//! extension authors declare it in their manifest and the SDK needs to
//! emit it without pulling in the runtime.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    Config,
    Input,
    Output,
    Status,
}

/// Declarative schema for a slot (value schema, direction, role).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSchema {
    pub name: String,
    pub role: SlotRole,
    /// JSON Schema for values written to this slot.
    #[serde(default)]
    pub value_schema: JsonValue,
    #[serde(default)]
    pub writable: bool,
    /// Input slots only: whether a write to this slot causes the node's
    /// `NodeBehavior::on_message` to fire. Non-trigger inputs accumulate
    /// state for the next trigger to read. Stage 3a-2 supports trigger
    /// on input slots; the field is ignored on other roles.
    #[serde(default)]
    pub trigger: bool,
}

impl SlotSchema {
    pub fn new(name: impl Into<String>, role: SlotRole) -> Self {
        Self {
            name: name.into(),
            role,
            value_schema: JsonValue::Object(Default::default()),
            writable: false,
            trigger: false,
        }
    }

    pub fn writable(mut self) -> Self {
        self.writable = true;
        self
    }

    pub fn with_schema(mut self, schema: JsonValue) -> Self {
        self.value_schema = schema;
        self
    }

    pub fn triggers(mut self) -> Self {
        self.trigger = true;
        self
    }
}
