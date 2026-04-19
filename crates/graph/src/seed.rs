//! Seed kinds registered by every agent at startup.
//!
//! A handful of first-party kinds prove the substrate:
//!
//! * `acme.core.station` — the root container (one-per-graph)
//! * `acme.core.folder` — free container
//! * `acme.compute.math.add` — free leaf (native compute, for Stage 3)
//! * `acme.driver.demo`, `.device`, `.point` — a demo bound-kind trio
//!   that proves placement rules work end-to-end.

use serde_json::json;

use crate::containment::{Cardinality, CascadePolicy, ContainmentSchema, ParentMatcher};
use crate::facets::{Facet, FacetSet};
use crate::ids::KindId;
use crate::kind::{KindManifest, KindRegistry};
use crate::slot::{SlotRole, SlotSchema};

/// Register the built-in kinds on the given registry.
pub fn register_builtins(kinds: &KindRegistry) {
    kinds.register(station());
    kinds.register(folder());
    kinds.register(math_add());
    kinds.register(driver_demo());
    kinds.register(driver_demo_device());
    kinds.register(driver_demo_point());
}

pub fn station() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.core.station"),
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
    .with_facets(FacetSet::of([Facet::IsSystem, Facet::IsContainer]))
}

pub fn folder() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.core.folder"),
        ContainmentSchema::default().with_may_contain([
            ParentMatcher::Facet(Facet::IsContainer),
            ParentMatcher::Facet(Facet::IsDriver),
            ParentMatcher::Facet(Facet::IsCompute),
            ParentMatcher::Facet(Facet::IsFlow),
        ]),
    )
    .with_display_name("Folder")
    .with_facets(FacetSet::of([Facet::IsContainer]))
}

pub fn math_add() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.compute.math.add"),
        ContainmentSchema::free_leaf(),
    )
    .with_display_name("Add")
    .with_facets(FacetSet::of([Facet::IsCompute]))
    .with_slots(vec![
        SlotSchema::new("a", SlotRole::Input).with_schema(json!({"type": "number"})),
        SlotSchema::new("b", SlotRole::Input).with_schema(json!({"type": "number"})),
        SlotSchema::new("sum", SlotRole::Output).with_schema(json!({"type": "number"})),
    ])
}

pub fn driver_demo() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.driver.demo"),
        ContainmentSchema::bound_under([
            ParentMatcher::Kind(KindId::new("acme.core.station")),
            ParentMatcher::Kind(KindId::new("acme.core.folder")),
        ])
        .with_may_contain([ParentMatcher::Kind(KindId::new("acme.driver.demo.device"))])
        .with_cardinality(Cardinality::ManyPerParent),
    )
    .with_display_name("Demo Driver")
    .with_facets(FacetSet::of([
        Facet::IsProtocol,
        Facet::IsDriver,
        Facet::IsContainer,
    ]))
}

pub fn driver_demo_device() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.driver.demo.device"),
        ContainmentSchema::bound_under([ParentMatcher::Kind(KindId::new("acme.driver.demo"))])
            .with_may_contain([ParentMatcher::Kind(KindId::new("acme.driver.demo.point"))]),
    )
    .with_display_name("Demo Device")
    .with_facets(FacetSet::of([Facet::IsDevice, Facet::IsContainer]))
}

pub fn driver_demo_point() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.driver.demo.point"),
        ContainmentSchema::bound_under([ParentMatcher::Kind(KindId::new(
            "acme.driver.demo.device",
        ))]),
    )
    .with_display_name("Demo Point")
    .with_facets(FacetSet::of([Facet::IsPoint, Facet::IsWritable]))
    .with_slots(vec![SlotSchema::new("value", SlotRole::Output)
        .writable()
        .with_schema(json!({"type": "number"}))])
}
