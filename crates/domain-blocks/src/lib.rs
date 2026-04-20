//! Extension-related node kinds.
//!
//! Currently ships one kind: [`Block`] (`sys.agent.block`). Per
//! `docs/design/EVERYTHING-AS-NODE.md` § "The agent itself is a node
//! too", block state lives on graph nodes — not a parallel registry —
//! so Studio subscribes to `graph.<tenant>.agent.blocks.*` the same
//! way it subscribes to every other node.

use blocks_sdk::NodeKind;

use graph::KindRegistry;

pub fn register_kinds(kinds: &KindRegistry) {
    kinds.register(<Block as NodeKind>::manifest());
}

#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.agent.block",
    manifest = "manifests/block.yaml",
    behavior = "none"
)]
pub struct Block;
