//! Layer loaders: file + environment.
//!
//! Each function returns an [`AgentConfigOverlay`]. The caller
//! (typically the binary's composition root) stacks them in the
//! documented precedence order:
//!
//! ```text
//! defaults < file < env < cli
//! ```

use std::path::{Path, PathBuf};

use crate::error::ConfigError;
use crate::model::{AgentConfigOverlay, DatabaseOverlay, LogOverlay, PluginsOverlay};
use crate::role::Role;

/// Read a YAML file. Returns an error if the file is absent or
/// malformed; missing-path callers should check with `path.exists()`
/// before dispatching.
pub fn from_file(path: &Path) -> Result<AgentConfigOverlay, ConfigError> {
    let bytes = std::fs::read(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yml::from_slice(&bytes).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Read the documented env subset. Names kept short and stable:
///
/// | Var | Field |
/// |---|---|
/// | `AGENT_ROLE` | `role` |
/// | `AGENT_DB` | `database.path` |
/// | `AGENT_LOG` | `log.filter` |
/// | `AGENT_PLUGINS_DIR` | `plugins.dir` |
///
/// Empty strings are treated as unset. Parsing failures for typed
/// fields (role) return an error rather than silently falling back.
pub fn from_env() -> Result<AgentConfigOverlay, ConfigError> {
    let role = match read_env("AGENT_ROLE")? {
        Some(s) => Some(
            s.parse::<Role>()
                .map_err(|e| ConfigError::Invalid(e.to_string()))?,
        ),
        None => None,
    };
    let db_path = read_env("AGENT_DB")?.map(PathBuf::from);
    let log_filter = read_env("AGENT_LOG")?;
    let plugins_dir = read_env("AGENT_PLUGINS_DIR")?.map(PathBuf::from);
    Ok(AgentConfigOverlay {
        role,
        database: db_path.map(|p| DatabaseOverlay { path: Some(p) }),
        log: log_filter.map(|f| LogOverlay { filter: Some(f) }),
        plugins: plugins_dir.map(|d| PluginsOverlay { dir: Some(d) }),
    })
}

fn read_env(key: &str) -> Result<Option<String>, ConfigError> {
    match std::env::var(key) {
        Ok(s) if s.is_empty() => Ok(None),
        Ok(s) => Ok(Some(s)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(ConfigError::EnvEncoding(key.to_string())),
    }
}
