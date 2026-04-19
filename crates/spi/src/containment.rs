//! Containment schema — the rules that keep the tree sane.
//!
//! Every kind declares what may live under it and where it may itself
//! live. The graph service enforces this on every mutation — one code
//! path covering CRUD, move, and extension-driven sync. The types live
//! here so extension authors can declare containment without pulling in
//! the graph runtime.

use serde::de::Error as DeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::facets::{Facet, FacetSet};
use crate::ids::KindId;

/// How to match a potential parent kind: by exact kind id, or by facet.
///
/// Serialises as a one-key map so containment lists read naturally in
/// YAML:
///
/// ```yaml
/// must_live_under:
///   - kind: acme.core.station
///   - facet: isContainer
/// ```
///
/// Implemented by hand (not derived) because external tagging on a
/// tuple variant with a transparent-string inner type doesn't round-trip
/// through `serde_yml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParentMatcher {
    Kind(KindId),
    Facet(Facet),
}

impl Serialize for ParentMatcher {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(1))?;
        match self {
            ParentMatcher::Kind(k) => map.serialize_entry("kind", k)?,
            ParentMatcher::Facet(f) => map.serialize_entry("facet", f)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ParentMatcher {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            kind: Option<KindId>,
            #[serde(default)]
            facet: Option<Facet>,
        }
        let h = Helper::deserialize(de)?;
        match (h.kind, h.facet) {
            (Some(k), None) => Ok(ParentMatcher::Kind(k)),
            (None, Some(f)) => Ok(ParentMatcher::Facet(f)),
            (Some(_), Some(_)) => Err(D::Error::custom(
                "ParentMatcher must set exactly one of `kind` or `facet`, not both",
            )),
            (None, None) => Err(D::Error::custom(
                "ParentMatcher must set either `kind` or `facet`",
            )),
        }
    }
}

impl ParentMatcher {
    /// True if this matcher applies to a parent with the given kind/facets.
    /// Used by the graph's placement validator and the kind-derive macro.
    pub fn matches(&self, parent_kind: &KindId, parent_facets: &FacetSet) -> bool {
        match self {
            ParentMatcher::Kind(k) => k == parent_kind,
            ParentMatcher::Facet(f) => parent_facets.contains(*f),
        }
    }
}

impl From<KindId> for ParentMatcher {
    fn from(k: KindId) -> Self {
        Self::Kind(k)
    }
}

impl From<&str> for ParentMatcher {
    fn from(k: &str) -> Self {
        Self::Kind(KindId::new(k))
    }
}

impl From<Facet> for ParentMatcher {
    fn from(f: Facet) -> Self {
        Self::Facet(f)
    }
}

/// How many children of a given kind a parent may hold.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    #[default]
    ManyPerParent,
    OnePerParent,
    ExactlyOne,
}

/// What happens when an instance of this kind is deleted while non-empty.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CascadePolicy {
    /// Delete the whole subtree transactionally.
    #[default]
    Strict,
    /// Refuse the delete if the subtree is non-empty.
    Deny,
    /// Leave children orphaned (rare — detached to lost-and-found).
    Orphan,
}

/// Per-kind containment rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainmentSchema {
    /// Kinds / facets under which this kind may be placed.
    /// Empty = *free* (place anywhere).
    #[serde(default)]
    pub must_live_under: Vec<ParentMatcher>,

    /// Kinds / facets this kind may hold as children.
    /// Empty = *leaf*.
    #[serde(default)]
    pub may_contain: Vec<ParentMatcher>,

    #[serde(default)]
    pub cardinality_per_parent: Cardinality,

    #[serde(default)]
    pub cascade: CascadePolicy,
}

impl ContainmentSchema {
    /// Free node: lives anywhere, holds nothing.
    pub fn free_leaf() -> Self {
        Self::default()
    }

    /// Convenience for `must_live_under = [parent_kind]`.
    pub fn bound_under(parents: impl IntoIterator<Item = ParentMatcher>) -> Self {
        Self {
            must_live_under: parents.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn with_may_contain(mut self, children: impl IntoIterator<Item = ParentMatcher>) -> Self {
        self.may_contain = children.into_iter().collect();
        self
    }

    pub fn with_cascade(mut self, c: CascadePolicy) -> Self {
        self.cascade = c;
        self
    }

    pub fn with_cardinality(mut self, c: Cardinality) -> Self {
        self.cardinality_per_parent = c;
        self
    }

    pub fn is_free(&self) -> bool {
        self.must_live_under.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_matcher_round_trips_through_yaml() {
        for m in [
            ParentMatcher::Kind(KindId::new("acme.core.station")),
            ParentMatcher::Facet(Facet::IsContainer),
        ] {
            let y = serde_yml::to_string(&m).unwrap();
            let back: ParentMatcher = serde_yml::from_str(&y).unwrap();
            assert_eq!(back, m, "round-trip failed for {m:?}: {y}");
        }
    }

    #[test]
    fn parent_matcher_rejects_both_keys() {
        let bad = "kind: foo\nfacet: isContainer\n";
        assert!(serde_yml::from_str::<ParentMatcher>(bad).is_err());
    }

    #[test]
    fn parent_matcher_rejects_neither_key() {
        let bad = "other: x\n";
        assert!(serde_yml::from_str::<ParentMatcher>(bad).is_err());
    }
}
