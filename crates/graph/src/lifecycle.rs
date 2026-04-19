//! Per-node lifecycle state machine.
//!
//! The full state set is designed to cover everything from human-created
//! nodes to extension-backed nodes that can go stale when an extension
//! crashes. Transitions are explicit; illegal transitions return `None`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Freshly created, not yet activated.
    Created,
    /// Operating normally.
    Active,
    /// Intentionally stopped; resumable.
    Disabled,
    /// An upstream (extension/process) has crashed; values are untrusted
    /// but the node remains addressable.
    Stale,
    /// A protocol or validation failure is preventing operation.
    Fault,
    /// About to be removed; used briefly during delete.
    Removing,
    /// Terminal.
    Removed,
}

impl Lifecycle {
    /// Whether moving from `self` to `next` is legal. Symmetric only
    /// where the state machine genuinely is (`Disabled`↔`Active`).
    pub fn can_transition_to(self, next: Lifecycle) -> bool {
        use Lifecycle::*;
        match (self, next) {
            (a, b) if a == b => false,
            (Created, Active | Disabled | Fault | Removing) => true,
            (Active, Disabled | Stale | Fault | Removing) => true,
            (Disabled, Active | Removing | Fault) => true,
            (Stale, Active | Fault | Removing) => true,
            (Fault, Active | Disabled | Removing) => true,
            (Removing, Removed) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_paths() {
        assert!(Lifecycle::Created.can_transition_to(Lifecycle::Active));
        assert!(Lifecycle::Active.can_transition_to(Lifecycle::Disabled));
        assert!(Lifecycle::Disabled.can_transition_to(Lifecycle::Active));
        assert!(Lifecycle::Active.can_transition_to(Lifecycle::Stale));
        assert!(Lifecycle::Removing.can_transition_to(Lifecycle::Removed));
    }

    #[test]
    fn terminal_has_no_exit() {
        for next in [
            Lifecycle::Created,
            Lifecycle::Active,
            Lifecycle::Disabled,
            Lifecycle::Stale,
            Lifecycle::Fault,
            Lifecycle::Removing,
            Lifecycle::Removed,
        ] {
            assert!(!Lifecycle::Removed.can_transition_to(next));
        }
    }
}
