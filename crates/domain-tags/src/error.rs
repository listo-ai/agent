use thiserror::Error;

/// Errors produced by tag validation and shorthand parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TagsError {
    #[error("tag label contains invalid characters: `{0}`")]
    InvalidLabelChars(String),

    #[error("tag key contains invalid characters: `{0}`")]
    InvalidKeyChars(String),

    #[error("tag value is empty for key `{0}`")]
    EmptyValue(String),

    #[error("tag value for key `{0}` exceeds max length of 128")]
    ValueTooLong(String),

    #[error("tag value contains control characters for key `{0}`")]
    ControlCharInValue(String),

    #[error("key `{0}` is in the reserved `sys.*` namespace")]
    ReservedKey(String),

    #[error("too many labels: max 64, got {0}")]
    TooManyLabels(usize),

    #[error("too many kv entries: max 64, got {0}")]
    TooManyKv(usize),

    #[error("key length exceeds 64 for key `{0}`")]
    KeyTooLong(String),

    #[error("malformed tags JSON: expected an object with `labels` and `kv` keys")]
    MalformedJson,

    #[error("malformed shorthand: {0}")]
    MalformedShorthand(String),
}
