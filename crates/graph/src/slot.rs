//! Typed slots on a node.
//!
//! Every slot has a **role** telling the platform how to treat it:
//! `config` (user-authored, persisted, audit-worthy), `input`/`output`
//! (live data flow), or `status` (platform-computed state). See
//! EVERYTHING-AS-NODE.md § "Slot roles".

use std::collections::BTreeMap;

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

/// Declarative schema for a slot (what value-schemas, direction, role).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSchema {
    pub name: String,
    pub role: SlotRole,
    /// JSON Schema for values written to this slot.
    #[serde(default)]
    pub value_schema: JsonValue,
    #[serde(default)]
    pub writable: bool,
}

impl SlotSchema {
    pub fn new(name: impl Into<String>, role: SlotRole) -> Self {
        Self {
            name: name.into(),
            role,
            value_schema: JsonValue::Object(Default::default()),
            writable: false,
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
}

/// A slot's live value plus a monotonic generation counter so
/// subscribers can tell stale events from fresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotValue {
    pub value: JsonValue,
    pub generation: u64,
}

impl SlotValue {
    pub fn new(value: JsonValue) -> Self {
        Self {
            value,
            generation: 0,
        }
    }

    pub(crate) fn bump(&mut self, new_value: JsonValue) {
        self.value = new_value;
        self.generation += 1;
    }
}

/// Map of slot-name → current value for a node.
#[derive(Debug, Clone, Default)]
pub struct SlotMap {
    inner: BTreeMap<String, SlotValue>,
}

impl SlotMap {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)] // Public read API; consumers land in later stages.
    pub fn get(&self, name: &str) -> Option<&SlotValue> {
        self.inner.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.inner.contains_key(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, value: JsonValue) {
        self.inner.insert(name.into(), SlotValue::new(value));
    }

    pub(crate) fn write(&mut self, name: &str, value: JsonValue) -> Option<u64> {
        let slot = self.inner.get_mut(name)?;
        slot.bump(value);
        Some(slot.generation)
    }

    /// Seed a slot with a specific value and generation. Used by
    /// [`crate::persist`] during startup restoration \u{2014} no event fires
    /// because no user-facing mutation is happening.
    pub(crate) fn restore(&mut self, name: impl Into<String>, value: JsonValue, generation: u64) {
        self.inner
            .insert(name.into(), SlotValue { value, generation });
    }

    /// The current generation of the named slot, or `None` if the slot
    /// isn't declared. Used by the write-through path to compute the
    /// next generation for the repo call before committing to memory.
    pub(crate) fn current_generation(&self, name: &str) -> Option<u64> {
        self.inner.get(name).map(|sv| sv.generation)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &SlotValue)> {
        self.inner.iter()
    }

    #[allow(dead_code)] // Public read API; consumers land in later stages.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
