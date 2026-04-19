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

/// Primitive type of a slot's value — the single source of truth used
/// by the historizer to decide which storage table to write into and
/// which COV semantics to apply.
///
/// Stored as the `slots.kind` column (denormalization for query
/// filters); the historizer reads it from the kind registry built at
/// boot, so the column's population state never drives behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotValueKind {
    /// Structureless absence-of-value (`null`).
    #[default]
    Null,
    /// Boolean flag. Historized in time-series tables (scalar).
    Bool,
    /// IEEE-754 double. Historized in time-series tables (scalar).
    Number,
    /// UTF-8 text of arbitrary length. Historized in `slot_history`.
    String,
    /// Arbitrary JSON document. Historized in `slot_history`.
    Json,
    /// Raw bytes. Historized in `slot_history` as a BLOB.
    Binary,
}

/// Declarative schema for a slot (value schema, direction, role).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSchema {
    pub name: String,
    pub role: SlotRole,
    /// Primitive value kind. Drives historizer table routing and COV
    /// semantics. Defaults to `Json` for backwards compatibility with
    /// slots declared before this field existed.
    #[serde(default = "SlotValueKind::default_json")]
    pub value_kind: SlotValueKind,
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

impl SlotValueKind {
    /// Serde default function — yields `Json` so slots without an
    /// explicit `value_kind` retain their pre-history behaviour.
    pub fn default_json() -> Self {
        Self::Json
    }

    /// Returns the stable lower-snake string written to `slots.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Number => "number",
            Self::String => "string",
            Self::Json => "json",
            Self::Binary => "binary",
        }
    }

    /// Whether this kind is routed to the time-series tables (`Bool` /
    /// `Number`). Otherwise it goes to `slot_history`.
    pub fn is_scalar(self) -> bool {
        matches!(self, Self::Bool | Self::Number)
    }
}

impl SlotSchema {
    pub fn new(name: impl Into<String>, role: SlotRole) -> Self {
        Self {
            name: name.into(),
            role,
            value_kind: SlotValueKind::Json,
            value_schema: JsonValue::Object(Default::default()),
            writable: false,
            trigger: false,
        }
    }

    pub fn with_kind(mut self, kind: SlotValueKind) -> Self {
        self.value_kind = kind;
        self
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
