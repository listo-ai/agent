//! Engine-contributed kinds.
//!
//! The engine owns three kinds that together express "a running flow":
//!
//! * `acme.core.flow` \u{2014} a flow document container. Lives under any
//!   other container; holds the flow's internal nodes. Facet `IsFlow`
//!   wires it into palette grouping and RBAC.
//! * `acme.engine.read_slot` \u{2014} a flow-internal node that subscribes
//!   to an external slot and emits its value onto a wire.
//! * `acme.engine.write_slot` \u{2014} a flow-internal node that writes a
//!   wire value back to an external slot.
//!
//! In Stage 2 only the kinds themselves are registered; execution of
//! flow documents arrives in Stage 2b once crossflow is vendored per
//! `docs/design/RUNTIME.md` § "What crossflow is". The **live-wire
//! executor** already works without any of these kinds \u{2014} it reads and
//! writes slots directly on the graph.

use graph::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
    KindRegistry, ParentMatcher, SlotRole, SlotSchema,
};
use serde_json::json;

/// Register every engine-contributed kind.
pub fn register(kinds: &KindRegistry) {
    kinds.register(flow());
    kinds.register(read_slot());
    kinds.register(write_slot());
}

pub fn flow() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.core.flow"),
        ContainmentSchema::default()
            .with_may_contain([
                ParentMatcher::Facet(Facet::IsCompute),
                ParentMatcher::Kind(KindId::new("acme.engine.read_slot")),
                ParentMatcher::Kind(KindId::new("acme.engine.write_slot")),
                ParentMatcher::Kind(KindId::new("acme.core.flow")),
            ])
            .with_cardinality(Cardinality::ManyPerParent)
            .with_cascade(CascadePolicy::Strict),
    )
    .with_display_name("Flow")
    .with_facets(FacetSet::of([Facet::IsFlow, Facet::IsContainer]))
}

pub fn read_slot() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.engine.read_slot"),
        ContainmentSchema::bound_under([ParentMatcher::Facet(Facet::IsFlow)]),
    )
    .with_display_name("Read Slot")
    .with_facets(FacetSet::of([Facet::IsCompute]))
    .with_slots(vec![
        // User-authored target: which slot to subscribe to.
        SlotSchema::new("target_path", SlotRole::Config).with_schema(json!({"type": "string"})),
        SlotSchema::new("target_slot", SlotRole::Config).with_schema(json!({"type": "string"})),
        // Emitted value \u{2014} wired to downstream compute nodes.
        SlotSchema::new("value", SlotRole::Output),
    ])
}

pub fn write_slot() -> KindManifest {
    KindManifest::new(
        KindId::new("acme.engine.write_slot"),
        ContainmentSchema::bound_under([ParentMatcher::Facet(Facet::IsFlow)]),
    )
    .with_display_name("Write Slot")
    .with_facets(FacetSet::of([Facet::IsCompute]))
    .with_slots(vec![
        SlotSchema::new("target_path", SlotRole::Config).with_schema(json!({"type": "string"})),
        SlotSchema::new("target_slot", SlotRole::Config).with_schema(json!({"type": "string"})),
        // Incoming value \u{2014} wired from upstream.
        SlotSchema::new("value", SlotRole::Input).writable(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_register() {
        let kinds = KindRegistry::new();
        register(&kinds);
        for id in [
            "acme.core.flow",
            "acme.engine.read_slot",
            "acme.engine.write_slot",
        ] {
            assert!(kinds.contains(&KindId::new(id)), "missing {id}");
        }
    }

    #[test]
    fn flow_carries_is_flow_facet() {
        assert!(flow().facets.contains(Facet::IsFlow));
    }
}
