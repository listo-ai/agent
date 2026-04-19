//! Containment schema — the rules that keep the tree sane.
//!
//! Every kind declares what may live under it and where it may itself
//! live. The graph service enforces this on every mutation — one code
//! path covering CRUD, move, and extension-driven sync.

use serde::{Deserialize, Serialize};

use crate::facets::{Facet, FacetSet};
use crate::ids::KindId;

/// How to match a potential parent kind: by exact kind id, or by facet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum ParentMatcher {
    Kind(KindId),
    Facet(Facet),
}

impl ParentMatcher {
    pub(crate) fn matches(&self, parent_kind: &KindId, parent_facets: &FacetSet) -> bool {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    ManyPerParent,
    OnePerParent,
    ExactlyOne,
}

impl Default for Cardinality {
    fn default() -> Self {
        Cardinality::ManyPerParent
    }
}

/// What happens when an instance of this kind is deleted while non-empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CascadePolicy {
    /// Delete the whole subtree transactionally.
    Strict,
    /// Refuse the delete if the subtree is non-empty.
    Deny,
    /// Leave children orphaned (rare — detached to lost-and-found).
    Orphan,
}

impl Default for CascadePolicy {
    fn default() -> Self {
        CascadePolicy::Strict
    }
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
