#![allow(clippy::unwrap_used, clippy::panic)]
//! Snapshot regression tests for engine-contributed kinds.
//!
//! Stage 3a-1 moved these from hand-built Rust values to YAML
//! manifests loaded via `#[derive(NodeKind)]`. The tests here pin the
//! pre-migration values; a YAML edit that drifts the kind surface
//! lands as a diff against this file. See
//! `crates/graph/tests/seed_snapshot.rs` for the matching regression
//! gate on seed kinds.

use engine::kinds::{Flow, ReadSlot, WriteSlot};
use extensions_sdk::NodeKind;
use serde_json::json;
use spi::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
    ParentMatcher, SlotRole, SlotSchema,
};

#[track_caller]
fn assert_manifest_eq(actual: KindManifest, expected: KindManifest) {
    let a = serde_json::to_value(&actual).unwrap();
    let e = serde_json::to_value(&expected).unwrap();
    assert_eq!(a, e);
}

#[test]
fn flow_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.core.flow"),
        ContainmentSchema::default()
            .with_may_contain([
                ParentMatcher::Facet(Facet::IsCompute),
                ParentMatcher::Kind(KindId::new("sys.engine.read_slot")),
                ParentMatcher::Kind(KindId::new("sys.engine.write_slot")),
                ParentMatcher::Kind(KindId::new("sys.core.flow")),
                ParentMatcher::Kind(KindId::new("ui.nav")),
                ParentMatcher::Kind(KindId::new("ui.page")),
                ParentMatcher::Kind(KindId::new("ui.template")),
                ParentMatcher::Kind(KindId::new("ui.widget")),
            ])
            .with_cardinality(Cardinality::ManyPerParent)
            .with_cascade(CascadePolicy::Strict),
    )
    .with_display_name("Flow")
    .with_facets(FacetSet::of([Facet::IsFlow, Facet::IsContainer]));
    assert_manifest_eq(<Flow as NodeKind>::manifest(), expected);
}

#[test]
fn read_slot_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.engine.read_slot"),
        ContainmentSchema::bound_under([ParentMatcher::Facet(Facet::IsFlow)]),
    )
    .with_display_name("Read Slot")
    .with_facets(FacetSet::of([Facet::IsCompute]))
    .with_slots(vec![
        SlotSchema::new("target_path", SlotRole::Config).with_schema(json!({"type": "string"})),
        SlotSchema::new("target_slot", SlotRole::Config).with_schema(json!({"type": "string"})),
        SlotSchema::new("value", SlotRole::Output),
    ]);
    assert_manifest_eq(<ReadSlot as NodeKind>::manifest(), expected);
}

#[test]
fn write_slot_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.engine.write_slot"),
        ContainmentSchema::bound_under([ParentMatcher::Facet(Facet::IsFlow)]),
    )
    .with_display_name("Write Slot")
    .with_facets(FacetSet::of([Facet::IsCompute]))
    .with_slots(vec![
        SlotSchema::new("target_path", SlotRole::Config).with_schema(json!({"type": "string"})),
        SlotSchema::new("target_slot", SlotRole::Config).with_schema(json!({"type": "string"})),
        SlotSchema::new("value", SlotRole::Input).writable(),
    ]);
    assert_manifest_eq(<WriteSlot as NodeKind>::manifest(), expected);
}
