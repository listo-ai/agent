#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Dashboard node kinds — the two UI artifacts in the graph.
//!
//! * `ui.nav` — sidebar row / hierarchical nav entry.
//! * `ui.page` — authored SDUI page; `layout` slot holds a typed
//!   `ComponentTree` that `/api/v1/ui/resolve` deserializes and returns
//!   verbatim (plus a subscription plan). See `docs/design/SDUI.md`.
//!
//! Both kinds have `behavior = "none"` — pure graph state. Resolution
//! (binding resolver, render tree, subscription derivation) lives in
//! `dashboard-runtime` and `dashboard-transport`.

use blocks_sdk::NodeKind;

#[derive(NodeKind)]
#[node(kind = "ui.nav", manifest = "manifests/nav.yaml", behavior = "none")]
pub struct Nav;

#[derive(NodeKind)]
#[node(kind = "ui.page", manifest = "manifests/page.yaml", behavior = "none")]
pub struct Page;

/// Register every dashboard kind on a [`graph::KindRegistry`].
pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(<Nav as NodeKind>::manifest());
    kinds.register(<Page as NodeKind>::manifest());
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::KindId;

    #[test]
    fn register_ui_kinds() {
        let kinds = graph::KindRegistry::new();
        register_kinds(&kinds);
        for id in ["ui.nav", "ui.page"] {
            assert!(
                kinds.contains(&KindId::new(id)),
                "expected {id} to be registered",
            );
        }
    }
}
