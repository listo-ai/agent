//! Engine-wide state machine.
//!
//! Per `docs/design/RUNTIME.md` § "Engine state machine":
//! `Starting \u{2192} Running \u{2192} Pausing \u{2192} Paused \u{2192} Resuming \u{2192} Running \u{2192} Stopping \u{2192} Stopped`.
//!
//! Transitions are explicit; illegal transitions return `false` and the
//! caller surfaces an [`EngineError::IllegalTransition`](crate::error::EngineError).
//! Every transition is logged with `tracing` (done by the owner, not
//! here) so the operator can reconstruct the timeline.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// Initial state. The worker is not running.
    #[default]
    Stopped,
    /// `start()` in flight \u{2014} worker is spinning up.
    Starting,
    /// Worker live; live-wire propagation enabled.
    Running,
    /// `pause()` in flight \u{2014} new events rejected, in-flight drained.
    Pausing,
    /// Worker live but quiescent \u{2014} no propagation until `resume()`.
    Paused,
    /// `resume()` in flight.
    Resuming,
    /// `shutdown()` in flight \u{2014} safe-state being applied, worker draining.
    Stopping,
}

impl EngineState {
    pub fn can_transition_to(self, next: EngineState) -> bool {
        use EngineState::*;
        match (self, next) {
            (a, b) if a == b => false,
            (Stopped, Starting) => true,
            (Starting, Running | Stopping) => true,
            (Running, Pausing | Stopping) => true,
            (Pausing, Paused | Stopping) => true,
            (Paused, Resuming | Stopping) => true,
            (Resuming, Running | Stopping) => true,
            (Stopping, Stopped) => true,
            _ => false,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, EngineState::Stopped)
    }

    /// `true` when the engine is in a state that should propagate slot
    /// changes along live-wire links. Only `Running` qualifies.
    pub fn propagates(self) -> bool {
        matches!(self, EngineState::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_legal() {
        let seq = [
            EngineState::Stopped,
            EngineState::Starting,
            EngineState::Running,
            EngineState::Pausing,
            EngineState::Paused,
            EngineState::Resuming,
            EngineState::Running,
            EngineState::Stopping,
            EngineState::Stopped,
        ];
        for w in seq.windows(2) {
            assert!(
                w[0].can_transition_to(w[1]),
                "legal transition rejected: {:?} -> {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn self_transitions_rejected() {
        for s in [
            EngineState::Stopped,
            EngineState::Starting,
            EngineState::Running,
            EngineState::Pausing,
            EngineState::Paused,
            EngineState::Resuming,
            EngineState::Stopping,
        ] {
            assert!(!s.can_transition_to(s));
        }
    }

    #[test]
    fn running_is_the_only_propagating_state() {
        for s in [
            EngineState::Stopped,
            EngineState::Starting,
            EngineState::Pausing,
            EngineState::Paused,
            EngineState::Resuming,
            EngineState::Stopping,
        ] {
            assert!(!s.propagates(), "{s:?} must not propagate");
        }
        assert!(EngineState::Running.propagates());
    }

    #[test]
    fn cannot_skip_stopping_on_shutdown() {
        assert!(!EngineState::Running.can_transition_to(EngineState::Stopped));
        assert!(EngineState::Running.can_transition_to(EngineState::Stopping));
        assert!(EngineState::Stopping.can_transition_to(EngineState::Stopped));
    }
}
