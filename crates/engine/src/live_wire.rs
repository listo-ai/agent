//! Live-wire executor \u{2014} reactive slot\u{2192}slot propagation.
//!
//! Per `docs/sessions/STEPS.md` § "Stage 2", this is the
//! Niagara-style "simple case, no flow wrapper": when a source slot
//! changes, every link departing from it forwards the new value to its
//! target slot. No diagram, no flow container, no session \u{2014} just the
//! wire.
//!
//! This path exists alongside the flow-document executor (Stage 2b,
//! crossflow-based). A user who only needs a couple of reactive hops
//! never has to open the flow editor.
//!
//! ## Cycle handling
//!
//! We skip a propagation when the target slot already holds the
//! incoming value. This breaks fixed-point cycles (A\u{2192}B\u{2192}A carrying
//! the same value) without interfering with genuine fan-out. Non-fixed
//! cycles (A\u{2192}B where B transforms) are a flow-document concern and
//! execute through crossflow.

use std::sync::Arc;

use graph::{GraphEvent, GraphStore, SlotRef};

pub(crate) struct LiveWireExecutor {
    graph: Arc<GraphStore>,
}

impl LiveWireExecutor {
    pub(crate) fn new(graph: Arc<GraphStore>) -> Self {
        Self { graph }
    }

    /// Process a single graph event. Only `SlotChanged` triggers
    /// propagation \u{2014} other events (link adds, lifecycle moves) matter
    /// to later stages but not to the live-wire path.
    pub(crate) fn handle(&self, event: &GraphEvent) {
        let GraphEvent::SlotChanged {
            id,
            slot,
            value,
            generation,
            ..
        } = event
        else {
            return;
        };
        let source = SlotRef::new(*id, slot.clone());
        let links = self.graph.links_from(&source);
        if links.is_empty() {
            return;
        }
        tracing::trace!(
            source_node = %id, source_slot = %slot, generation,
            fan_out = links.len(), "live-wire propagation",
        );
        for link in links {
            let Some(target_node) = self.graph.get_by_id(link.target.node) else {
                tracing::debug!(link = %link.id.0, "live-wire: target node vanished, skipping");
                continue;
            };
            let equal = target_node
                .slot_values
                .iter()
                .find(|(name, _)| name == &link.target.slot)
                .map(|(_, sv)| sv.value == *value)
                .unwrap_or(false);
            if equal {
                tracing::trace!(link = %link.id.0, "live-wire: target already at value, skipping");
                continue;
            }
            if let Err(err) =
                self.graph
                    .write_slot(&target_node.path, &link.target.slot, value.clone())
            {
                tracing::warn!(
                    link = %link.id.0, error = %err,
                    "live-wire: target write rejected",
                );
            }
        }
    }
}
