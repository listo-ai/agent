//! Fleet remote-agent and group node kinds.
//!
//! Registers `sys.fleet.remote-agent` and `sys.fleet.group` so the
//! graph tree can hold discovered remotes without any parallel state.
//! See `docs/design/FLEET-TRANSPORT.md § "The remote-agent node"`.

use blocks_sdk::NodeKind;
use graph::KindRegistry;

/// Register both fleet kinds into the shared [`KindRegistry`].
pub fn register_kinds(kinds: &KindRegistry) {
    kinds.register(<RemoteAgent as NodeKind>::manifest());
    kinds.register(<Group as NodeKind>::manifest());
}

/// `sys.fleet.remote-agent` — one known remote agent.
///
/// Selecting this node in Studio switches the ambient scope to
/// `FleetScope::Remote { tenant, agent_id }` (sourced from the config
/// slots) and all subsequent `AgentClient` calls route via fleet
/// req/reply instead of local HTTP.
#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.fleet.remote-agent",
    manifest = "manifests/remote_agent.yaml",
    behavior = "none"
)]
pub struct RemoteAgent;

/// `sys.fleet.group` — container for grouping remote agents by
/// site, region, or tenant. Nested groups are allowed so operators
/// can build region → site → rack hierarchies.
#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.fleet.group",
    manifest = "manifests/group.yaml",
    behavior = "none"
)]
pub struct Group;

#[cfg(test)]
mod tests {
    use super::*;
    use spi::KindId;

    #[test]
    fn both_kinds_register_and_parse() {
        let kinds = graph::KindRegistry::new();
        register_kinds(&kinds);
        assert!(
            kinds.contains(&KindId::new("sys.fleet.remote-agent")),
            "sys.fleet.remote-agent not registered",
        );
        assert!(
            kinds.contains(&KindId::new("sys.fleet.group")),
            "sys.fleet.group not registered",
        );
    }

    #[test]
    fn remote_agent_is_not_a_container() {
        let kinds = graph::KindRegistry::new();
        register_kinds(&kinds);
        let m = kinds.get(&KindId::new("sys.fleet.remote-agent")).unwrap();
        assert!(
            !m.facets.contains(spi::Facet::IsContainer),
            "remote-agent must not be isContainer — child expansion is a scope switch, not containment",
        );
        assert!(m.containment.may_contain.is_empty());
    }

    #[test]
    fn group_is_a_container_and_accepts_remote_agents_and_nested_groups() {
        let kinds = graph::KindRegistry::new();
        register_kinds(&kinds);
        let m = kinds.get(&KindId::new("sys.fleet.group")).unwrap();
        assert!(m.facets.contains(spi::Facet::IsContainer));
        let children: Vec<_> = m
            .containment
            .may_contain
            .iter()
            .filter_map(|p| match p {
                spi::ParentMatcher::Kind(k) => Some(k.as_str().to_string()),
                spi::ParentMatcher::Facet(_) => None,
            })
            .collect();
        assert!(
            children.contains(&"sys.fleet.remote-agent".to_string()),
            "group must accept remote-agent children",
        );
        assert!(
            children.contains(&"sys.fleet.group".to_string()),
            "group must accept nested group children for region→site→rack hierarchies",
        );
    }
}
