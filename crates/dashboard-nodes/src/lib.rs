#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Dashboard node kinds — the four UI artifacts in the unified graph.
//!
//! See `docs/design/DASHBOARD.md` for the design. This crate ships M1:
//!
//! * `ui.nav`, `ui.template`, `ui.page`, `ui.widget` registered as kinds
//! * Parameter-contract validator (`ui.page.bound_args` vs.
//!   `ui.template.requires`)
//!
//! The kinds have `behavior = "none"` — they are pure graph state.
//! Resolution (context stack, binding resolver, render tree) lives in
//! `dashboard-runtime` (M2+).

use extensions_sdk::NodeKind;

pub mod contract;

pub use contract::{validate_bound_args, ContractError, ParamSpec, ParamType, Requires};

#[derive(NodeKind)]
#[node(
    kind = "ui.nav",
    manifest = "manifests/nav.yaml",
    behavior = "none"
)]
pub struct Nav;

#[derive(NodeKind)]
#[node(
    kind = "ui.template",
    manifest = "manifests/template.yaml",
    behavior = "none"
)]
pub struct Template;

#[derive(NodeKind)]
#[node(
    kind = "ui.page",
    manifest = "manifests/page.yaml",
    behavior = "none"
)]
pub struct Page;

#[derive(NodeKind)]
#[node(
    kind = "ui.widget",
    manifest = "manifests/widget.yaml",
    behavior = "none"
)]
pub struct Widget;

/// Register every dashboard kind on a [`graph::KindRegistry`].
pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(<Nav as NodeKind>::manifest());
    kinds.register(<Template as NodeKind>::manifest());
    kinds.register(<Page as NodeKind>::manifest());
    kinds.register(<Widget as NodeKind>::manifest());
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::KindId;

    #[test]
    fn register_all_four_kinds() {
        let kinds = graph::KindRegistry::new();
        register_kinds(&kinds);
        for id in ["ui.nav", "ui.template", "ui.page", "ui.widget"] {
            assert!(
                kinds.contains(&KindId::new(id)),
                "expected {id} to be registered",
            );
        }
    }

    #[test]
    fn widget_must_live_under_page_or_template() {
        let kinds = graph::KindRegistry::new();
        register_kinds(&kinds);
        let w = kinds.get(&KindId::new("ui.widget")).unwrap();
        let parents: Vec<String> = w
            .containment
            .must_live_under
            .iter()
            .map(|p| match p {
                spi::ParentMatcher::Kind(k) => k.as_str().to_string(),
                spi::ParentMatcher::Facet(f) => format!("facet:{f:?}"),
            })
            .collect();
        assert!(parents.iter().any(|p| p == "ui.page"));
        assert!(parents.iter().any(|p| p == "ui.template"));
    }
}
