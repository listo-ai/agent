use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessagingError {
    #[error("bus closed")]
    Closed,
    #[error("backend error: {0}")]
    Backend(String),
}
