//! Containment / placement rules — the **one** place the "can this
//! child kind live under that parent?" check is implemented.
//!
//! Two callers consume it and must behave identically:
//!
//! 1. [`crate::GraphStore::create_child`] — enforces the rule at write
//!    time; a placement violation is a `PlacementRejected` error.
//! 2. `transport-rest`'s `/api/v1/kinds?placeable_under=<path>` — uses
//!    the same predicate to filter the palette the Studio offers.
//!
//! Keeping the rule as a pure function in this module means the two
//! surfaces can never drift. If you find yourself copying the loop
//! "`parent_manifest.containment.may_contain.iter().any(...)`" into a
//! new module, **stop** — that's this function's job. Call it.
//!
//! This is the exact scenario called out in
//! [`SKILLS/CODE-LAYOUT.md`](../../../../SKILLS/CODE-LAYOUT.md): domain
//! rules belong in the domain layer (here, in `graph`); transport
//! handlers orchestrate, they don't re-implement.

use spi::{Facet, KindId, KindManifest, ParentMatcher};

/// Can a node of `candidate` kind be placed under a node of
/// `parent_kind` (whose manifest is `parent_manifest`)?
///
/// Two-sided check — both sides must admit the pairing, with one
/// carve-out:
///
/// * **`may_contain`** on the parent: empty = unconstrained. Otherwise
///   at least one entry must match the candidate's kind or facets.
/// * **`must_live_under`** on the candidate: empty = unconstrained.
///   Otherwise at least one entry must match the parent.
/// * **`isAnywhere` bypass:** a candidate carrying `Facet::IsAnywhere`
///   skips the parent's `may_contain` whitelist. Rationale: some node
///   kinds (heartbeat, trigger, …) are intentionally placement-
///   agnostic and must not be locked out by older container manifests
///   that predate them. `must_live_under` still applies to them — an
///   `isAnywhere` node with a `must_live_under` constraint is still
///   constrained on the bottom side.
pub fn placement_allowed(
    parent_kind: &KindId,
    parent_manifest: &KindManifest,
    candidate: &KindManifest,
) -> bool {
    let may_contain_ok = candidate.facets.contains(Facet::IsAnywhere)
        || parent_manifest.containment.may_contain.is_empty()
        || parent_manifest
            .containment
            .may_contain
            .iter()
            .any(|m| match m {
                ParentMatcher::Kind(k) => k == &candidate.id,
                ParentMatcher::Facet(f) => candidate.facets.contains(*f),
            });

    let must_live_under_ok = candidate.containment.must_live_under.is_empty()
        || candidate
            .containment
            .must_live_under
            .iter()
            .any(|m| m.matches(parent_kind, &parent_manifest.facets));

    may_contain_ok && must_live_under_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{
        Cardinality, CascadePolicy, ContainmentSchema, FacetSet, KindId, KindManifest,
        ParentMatcher,
    };

    fn kind(id: &str, facets: FacetSet, containment: ContainmentSchema) -> KindManifest {
        KindManifest {
            id: KindId::new(id),
            display_name: None,
            facets,
            containment,
            slots: Vec::new(),
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        }
    }

    fn schema(must: Vec<ParentMatcher>, may: Vec<ParentMatcher>) -> ContainmentSchema {
        ContainmentSchema {
            must_live_under: must,
            may_contain: may,
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        }
    }

    #[test]
    fn empty_rules_on_both_sides_admits_everything() {
        let parent = kind("p", FacetSet::default(), schema(vec![], vec![]));
        let child = kind("c", FacetSet::default(), schema(vec![], vec![]));
        assert!(placement_allowed(&parent.id, &parent, &child));
    }

    #[test]
    fn may_contain_whitelist_rejects_unlisted_child() {
        let parent = kind(
            "driver",
            FacetSet::default(),
            schema(vec![], vec![ParentMatcher::Kind(KindId::new("driver.device"))]),
        );
        let allowed = kind("driver.device", FacetSet::default(), schema(vec![], vec![]));
        let denied = kind("sys.core.folder", FacetSet::default(), schema(vec![], vec![]));
        assert!(placement_allowed(&parent.id, &parent, &allowed));
        assert!(!placement_allowed(&parent.id, &parent, &denied));
    }

    #[test]
    fn may_contain_facet_matcher_admits_matching_child() {
        let parent = kind(
            "flow",
            FacetSet::default(),
            schema(vec![], vec![ParentMatcher::Facet(Facet::IsCompute)]),
        );
        let compute = kind("x", FacetSet::of([Facet::IsCompute]), schema(vec![], vec![]));
        let other = kind("y", FacetSet::of([Facet::IsDevice]), schema(vec![], vec![]));
        assert!(placement_allowed(&parent.id, &parent, &compute));
        assert!(!placement_allowed(&parent.id, &parent, &other));
    }

    #[test]
    fn must_live_under_bottom_side_check() {
        let parent = kind("flow", FacetSet::of([Facet::IsFlow]), schema(vec![], vec![]));
        let wrong_parent = kind("other", FacetSet::default(), schema(vec![], vec![]));

        let child = kind(
            "compute",
            FacetSet::default(),
            schema(vec![ParentMatcher::Facet(Facet::IsFlow)], vec![]),
        );

        assert!(placement_allowed(&parent.id, &parent, &child));
        assert!(!placement_allowed(&wrong_parent.id, &wrong_parent, &child));
    }

    #[test]
    fn is_anywhere_bypasses_may_contain_but_respects_must_live_under() {
        // Parent restricts children to a specific kind — would normally reject.
        let parent = kind(
            "strict",
            FacetSet::default(),
            schema(vec![], vec![ParentMatcher::Kind(KindId::new("approved"))]),
        );

        // Free `isAnywhere` node — bypasses `may_contain`.
        let free_anywhere = kind(
            "heartbeat",
            FacetSet::of([Facet::IsAnywhere]),
            schema(vec![], vec![]),
        );
        assert!(placement_allowed(&parent.id, &parent, &free_anywhere));

        // `isAnywhere` + `must_live_under: [isFlow]` — bottom side still applies.
        let scoped_anywhere = kind(
            "scoped",
            FacetSet::of([Facet::IsAnywhere]),
            schema(vec![ParentMatcher::Facet(Facet::IsFlow)], vec![]),
        );
        assert!(!placement_allowed(&parent.id, &parent, &scoped_anywhere));
    }
}
