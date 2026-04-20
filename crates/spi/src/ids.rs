//! Identifier types — small, cheap to clone, serialisable.
//!
//! These are part of the contract surface: block authors reference them
//! via the SDK's prelude. Keep them here (not in `graph`) so an extension
//! crate never transitively depends on the graph runtime.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Stable, opaque node identifier. Referenceable from anywhere in the
/// system (flows, links, audit, NATS subjects).
///
/// **Wire format is the un-hyphenated 32-char hex form** (uuid's
/// `simple()` representation). This is the one canonical string
/// shape across the whole stack — stored in the DB as-is, emitted
/// by `/api/v1/nodes`, carried in SSE `slot_changed` events,
/// addressed in subscription subjects, referenced from layout
/// bindings. `Display`, `Serialize`, and `to_string()` all return
/// the same thing. Deserialisation tolerates hyphenated input (for
/// paste-robustness and compatibility with externally-sourced
/// UUIDs) but immediately normalises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The single source of truth for NodeId string form. Anyone
        // going through Display, serde, `format!("{id}")`, or
        // `to_string()` gets the same 32-char un-hyphenated output.
        write!(f, "{}", self.0.simple())
    }
}

impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Never let serde fall through to Uuid's default (hyphenated)
        // serialiser. Route everything through the canonical form.
        s.collect_str(&self.0.simple())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // `Uuid::parse_str` accepts both hyphenated and simple forms,
        // so we can ingest either but always round-trip to simple.
        let raw = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Uuid::parse_str(&raw)
            .map(NodeId)
            .map_err(serde::de::Error::custom)
    }
}

impl FromStr for NodeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(NodeId)
    }
}

/// Reverse-DNS kind identifier (e.g. `sys.driver.bacnet.point`).
///
/// The kind is a *type*; the node path is a *location*. One kind can
/// have thousands of instances.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KindId(String);

impl KindId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KindId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for KindId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Materialised-path location of a node in the tree.
///
/// Paths are absolute and slash-separated: `/`, `/station`,
/// `/station/floor1/ahu-5`. The root is always `/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodePath(String);

impl NodePath {
    /// The root path (`/`).
    pub fn root() -> Self {
        Self("/".to_string())
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parent of this path, or `None` if root.
    pub fn parent(&self) -> Option<NodePath> {
        if self.is_root() {
            return None;
        }
        let trimmed = self.0.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(0) => Some(NodePath::root()),
            Some(i) => Some(NodePath(trimmed[..i].to_string())),
            None => None,
        }
    }

    /// Last segment of the path (the node's own name), or `"/"` for root.
    pub fn name(&self) -> &str {
        if self.is_root() {
            return "/";
        }
        self.0.rsplit('/').next().unwrap_or("")
    }

    /// Append a child name to this path. Returns `/` + name for root.
    pub fn child(&self, name: &str) -> NodePath {
        if self.is_root() {
            NodePath(format!("/{name}"))
        } else {
            NodePath(format!("{}/{name}", self.0))
        }
    }

    /// `true` if `self == other` or `self` is a proper ancestor of `other`.
    pub fn is_prefix_of(&self, other: &NodePath) -> bool {
        if self.is_root() {
            return true;
        }
        other.0 == self.0 || other.0.starts_with(&format!("{}/", self.0))
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for NodePath {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with('/') {
            return Err("paths must start with `/`");
        }
        Ok(Self(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_has_no_parent() {
        let r = NodePath::root();
        assert!(r.is_root());
        assert_eq!(r.parent(), None);
        assert_eq!(r.name(), "/");
    }

    #[test]
    fn child_and_parent_round_trip() {
        let p = NodePath::root().child("station").child("floor1");
        assert_eq!(p.as_str(), "/station/floor1");
        assert_eq!(p.name(), "floor1");
        assert_eq!(p.parent().unwrap().as_str(), "/station");
        assert_eq!(p.parent().unwrap().parent().unwrap(), NodePath::root());
    }

    #[test]
    fn prefix_matches_subtree_only() {
        let a = NodePath::root().child("a");
        let ab = a.child("b");
        let c = NodePath::root().child("c");
        assert!(NodePath::root().is_prefix_of(&a));
        assert!(a.is_prefix_of(&ab));
        assert!(!a.is_prefix_of(&c));
        let ab_sibling = NodePath::root().child("ab");
        assert!(!a.is_prefix_of(&ab_sibling));
    }
}
