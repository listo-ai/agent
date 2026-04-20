//! Engine-contributed kinds.
//!
//! The engine owns three kinds that together express "a running flow":
//!
//! * `sys.core.flow` — a flow document container. Lives under any
//!   other container; holds the flow's internal nodes. Facet `IsFlow`
//!   wires it into palette grouping and RBAC.
//! * `sys.engine.read_slot` — a flow-internal node that subscribes to
//!   an external slot and emits its value onto a wire.
//! * `sys.engine.write_slot` — a flow-internal node that writes a wire
//!   value back to an external slot.
//!
//! In Stage 2 only the kinds themselves are registered; execution of
//! flow documents arrives in Stage 2b once crossflow is vendored per
//! `docs/design/RUNTIME.md` § "What crossflow is". The live-wire
//! executor works without any of these kinds — it reads and writes
//! slots directly on the graph.
//!
//! Manifests are YAMLs under `manifests/` in this crate, wired via
//! `#[derive(NodeKind)]` so the SDK contract surface is the single
//! source of truth (Stage 3a-1).

use blocks_sdk::NodeKind;
use graph::KindRegistry;

/// Register every engine-contributed kind.
pub fn register(kinds: &KindRegistry) {
    kinds.register(<Flow as NodeKind>::manifest());
    kinds.register(<ReadSlot as NodeKind>::manifest());
    kinds.register(<WriteSlot as NodeKind>::manifest());
}

#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.core.flow",
    manifest = "manifests/flow.yaml",
    behavior = "none"
)]
pub struct Flow;

#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.engine.read_slot",
    manifest = "manifests/read_slot.yaml",
    behavior = "none"
)]
pub struct ReadSlot;

#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.engine.write_slot",
    manifest = "manifests/write_slot.yaml",
    behavior = "none"
)]
pub struct WriteSlot;

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{Facet, KindId};

    #[test]
    fn all_three_register() {
        let kinds = KindRegistry::new();
        register(&kinds);
        for id in [
            "sys.core.flow",
            "sys.engine.read_slot",
            "sys.engine.write_slot",
        ] {
            assert!(kinds.contains(&KindId::new(id)), "missing {id}");
        }
    }

    #[test]
    fn flow_carries_is_flow_facet() {
        let m = <Flow as NodeKind>::manifest();
        assert!(m.facets.contains(Facet::IsFlow));
    }
}
