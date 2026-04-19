use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),

    #[error("read config file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parse config file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yml::Error,
    },

    #[error("env var `{0}` is not valid UTF-8")]
    EnvEncoding(String),

    #[error("{0}")]
    Invalid(String),
}
