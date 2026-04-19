use thiserror::Error;

use spi::{KindId, NodePath};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GraphError {
    #[error("node not found at `{0}`")]
    NotFound(NodePath),

    #[error("kind `{0}` is not registered")]
    UnknownKind(KindId),

    #[error(
        "placement rejected: kind `{kind}` cannot live under `{parent}` (kind `{parent_kind}`)"
    )]
    PlacementRejected {
        kind: KindId,
        parent: NodePath,
        parent_kind: KindId,
    },

    #[error(
        "cardinality violated: `{parent}` already has the maximum number of `{kind}` children"
    )]
    CardinalityExceeded { kind: KindId, parent: NodePath },

    #[error("a node named `{name}` already exists under `{parent}`")]
    NameCollision { parent: NodePath, name: String },

    #[error("delete refused: kind `{kind}` at `{path}` has cascade=deny and is non-empty")]
    CascadeDenied { path: NodePath, kind: KindId },

    #[error("root already exists at `/`")]
    RootAlreadyExists,

    #[error("root must be created with no parent")]
    RootMustHaveNoParent,

    #[error("invalid node name `{0}` (must not be empty or contain `/`)")]
    InvalidNodeName(String),

    #[error("link endpoints must reference existing slots: {0}")]
    BadLink(String),

    #[error("persistence backend: {0}")]
    Backend(String),

    #[error("restore failed: {0}")]
    Restore(String),

    #[error("generation mismatch: expected {expected}, current {current}")]
    GenerationMismatch { expected: u64, current: u64 },
}
