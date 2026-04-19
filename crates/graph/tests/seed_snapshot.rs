#![allow(clippy::unwrap_used, clippy::panic)]
//! Snapshot regression tests for the seed kinds.
//!
//! Stage 3a-1 moved the seed kinds from hand-built `KindManifest`
//! values in Rust to YAML manifests loaded via `#[derive(NodeKind)]`.
//! These tests pin the pre-migration values so any YAML edit that
//! accidentally changes placement rules, facets, or slot schemas
//! surfaces as a diff against this file. Updating a manifest is a
//! deliberate act — if you mean to change one, update both sides.

use extensions_sdk::NodeKind;
use graph::seed::{DriverDemo, DriverDemoDevice, DriverDemoPoint, Folder, MathAdd, Station};
use serde_json::json;
use spi::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
    ParentMatcher, SlotRole, SlotSchema,
};

/// Compare two manifests via their JSON shape — `KindManifest` isn't
/// `PartialEq`, and JSON equality gives readable diffs on mismatch.
#[track_caller]
fn assert_manifest_eq(actual: KindManifest, expected: KindManifest) {
    let a = serde_json::to_value(&actual).unwrap();
    let e = serde_json::to_value(&expected).unwrap();
    assert_eq!(a, e);
}

#[test]
fn station_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.core.station"),
        ContainmentSchema::default()
            .with_may_contain([
                ParentMatcher::Facet(Facet::IsContainer),
                ParentMatcher::Facet(Facet::IsDriver),
                ParentMatcher::Facet(Facet::IsCompute),
                ParentMatcher::Facet(Facet::IsFlow),
            ])
            .with_cascade(CascadePolicy::Deny),
    )
    .with_display_name("Station")
    .with_facets(FacetSet::of([Facet::IsSystem, Facet::IsContainer]));
    assert_manifest_eq(<Station as NodeKind>::manifest(), expected);
}

#[test]
fn folder_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.core.folder"),
        ContainmentSchema::default().with_may_contain([
            ParentMatcher::Facet(Facet::IsContainer),
            ParentMatcher::Facet(Facet::IsDriver),
            ParentMatcher::Facet(Facet::IsCompute),
            ParentMatcher::Facet(Facet::IsFlow),
        ]),
    )
    .with_display_name("Folder")
    .with_facets(FacetSet::of([Facet::IsContainer]));
    assert_manifest_eq(<Folder as NodeKind>::manifest(), expected);
}

#[test]
fn math_add_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.compute.math.add"),
        ContainmentSchema::free_leaf(),
    )
    .with_display_name("Add")
    .with_facets(FacetSet::of([Facet::IsCompute]))
    .with_slots(vec![
        SlotSchema::new("a", SlotRole::Input).with_schema(json!({"type": "number"})),
        SlotSchema::new("b", SlotRole::Input).with_schema(json!({"type": "number"})),
        SlotSchema::new("sum", SlotRole::Output).with_schema(json!({"type": "number"})),
    ]);
    assert_manifest_eq(<MathAdd as NodeKind>::manifest(), expected);
}

#[test]
fn driver_demo_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.driver.demo"),
        ContainmentSchema::bound_under([
            ParentMatcher::Kind(KindId::new("sys.core.station")),
            ParentMatcher::Kind(KindId::new("sys.core.folder")),
        ])
        .with_may_contain([ParentMatcher::Kind(KindId::new("sys.driver.demo.device"))])
        .with_cardinality(Cardinality::ManyPerParent),
    )
    .with_display_name("Demo Driver")
    .with_facets(FacetSet::of([
        Facet::IsProtocol,
        Facet::IsDriver,
        Facet::IsContainer,
    ]));
    assert_manifest_eq(<DriverDemo as NodeKind>::manifest(), expected);
}

#[test]
fn driver_demo_device_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.driver.demo.device"),
        ContainmentSchema::bound_under([ParentMatcher::Kind(KindId::new("sys.driver.demo"))])
            .with_may_contain([ParentMatcher::Kind(KindId::new("sys.driver.demo.point"))]),
    )
    .with_display_name("Demo Device")
    .with_facets(FacetSet::of([Facet::IsDevice, Facet::IsContainer]));
    assert_manifest_eq(<DriverDemoDevice as NodeKind>::manifest(), expected);
}

#[test]
fn driver_demo_point_manifest_is_pinned() {
    let expected = KindManifest::new(
        KindId::new("sys.driver.demo.point"),
        ContainmentSchema::bound_under([ParentMatcher::Kind(KindId::new(
            "sys.driver.demo.device",
        ))]),
    )
    .with_display_name("Demo Point")
    .with_facets(FacetSet::of([Facet::IsPoint, Facet::IsWritable]))
    .with_slots(vec![SlotSchema::new("value", SlotRole::Output)
        .writable()
        .with_schema(json!({"type": "number"}))]);
    assert_manifest_eq(<DriverDemoPoint as NodeKind>::manifest(), expected);
}
