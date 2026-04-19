use data_repos::RepoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("sqlite: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("json encoding: {0}")]
    Json(#[from] serde_json::Error),

    #[error("migration: {0}")]
    Migration(String),
}

impl From<SqliteError> for RepoError {
    fn from(err: SqliteError) -> Self {
        match err {
            SqliteError::Rusqlite(rusqlite::Error::QueryReturnedNoRows) => RepoError::NotFound,
            other => RepoError::Backend(other.to_string()),
        }
    }
}
