use thiserror::Error;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("empty filter segment")]
    EmptyFilterSegment,
    #[error("invalid filter `{0}`")]
    InvalidFilter(String),
    #[error("invalid sort field `{0}`")]
    InvalidSort(String),
    #[error("unknown field `{0}`")]
    UnknownField(String),
    #[error("operator `{op:?}` is not allowed for field `{field}`")]
    UnsupportedOperator { field: String, op: crate::Operator },
    #[error("field `{field}` does not support operator `{op:?}`")]
    OperatorTypeMismatch { field: String, op: crate::Operator },
    #[error("page must be >= 1")]
    InvalidPage,
    #[error("size must be >= 1")]
    InvalidSize,
    #[error("size {requested} exceeds max page size {max}")]
    PageSizeTooLarge { requested: usize, max: usize },
    #[error("failed to serialize item for query evaluation: {0}")]
    Serialize(#[from] serde_json::Error),
}
