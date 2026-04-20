//! Links between slots — the "wires" shown in the Studio canvas.
//!
//! A link connects a source slot on one node to a target slot on
//! another (or the same) node. When either endpoint is deleted the link
//! is removed and a `LinkBroken` event fires on the surviving end —
//! never a silent disconnect. See EVERYTHING-AS-NODE.md § "Cascading
//! delete".

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use spi::NodeId;

/// Link identifier. Same wire contract as `NodeId`: un-hyphenated
/// 32-char hex form on serialize + Display, tolerant on deserialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(pub Uuid);

impl LinkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LinkId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for LinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.simple())
    }
}

impl Serialize for LinkId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0.simple())
    }
}

impl<'de> Deserialize<'de> for LinkId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Uuid::parse_str(&raw)
            .map(LinkId)
            .map_err(serde::de::Error::custom)
    }
}

impl FromStr for LinkId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(LinkId)
    }
}

/// Reference to a specific slot on a specific node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotRef {
    pub node: NodeId,
    pub slot: String,
}

impl SlotRef {
    pub fn new(node: NodeId, slot: impl Into<String>) -> Self {
        Self {
            node,
            slot: slot.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: LinkId,
    pub source: SlotRef,
    pub target: SlotRef,
}

impl Link {
    pub fn new(source: SlotRef, target: SlotRef) -> Self {
        Self {
            id: LinkId::new(),
            source,
            target,
        }
    }
}
