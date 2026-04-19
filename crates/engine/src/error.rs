use thiserror::Error;

use crate::state::EngineState;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("illegal engine transition: {from:?} \u{2192} {to:?}")]
    IllegalTransition { from: EngineState, to: EngineState },

    #[error("engine has no graph attached; call `attach` before `start`")]
    NoGraph,

    #[error("graph operation failed: {0}")]
    Graph(#[from] graph::GraphError),

    #[error("engine already started")]
    AlreadyStarted,

    #[error("worker task panicked")]
    WorkerPanicked,
}
