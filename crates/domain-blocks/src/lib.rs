//! Extension-related node kinds.
//!
//! Currently ships one kind: [`Plugin`] (`sys.agent.plugin`). Per
//! `docs/design/EVERYTHING-AS-NODE.md` § "The agent itself is a node
//! too", plugin state lives on graph nodes — not a parallel registry —
//! so Studio subscribes to `graph.<tenant>.agent.plugins.*` the same
//! way it subscribes to every other node.

use extensions_sdk::NodeKind;

use graph::KindRegistry;

pub fn register_kinds(kinds: &KindRegistry) {
    kinds.register(<Plugin as NodeKind>::manifest());
}

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "sys.agent.plugin",
    manifest = "manifests/plugin.yaml",
    behavior = "none"
)]
pub struct Plugin;
