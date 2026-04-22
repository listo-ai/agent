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
/// | `AGENT_BLOCKS_DIR` | `blocks.dir` |
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
    let blocks_dir = read_env("AGENT_BLOCKS_DIR")?.map(PathBuf::from);
    Ok(AgentConfigOverlay {
        role,
        database: db_path.map(|p| DatabaseOverlay { path: Some(p) }),
        log: log_filter.map(|f| LogOverlay { filter: Some(f) }),
        blocks: blocks_dir.map(|d| PluginsOverlay { dir: Some(d) }),
        fleet: None,
        auth: None,
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

/// Errors from [`to_file`]. Kept separate from [`ConfigError`]
/// because the write path has failure modes (permission tightening,
/// atomic rename) that the read path does not.
#[derive(Debug, thiserror::Error)]
pub enum WriteBackError {
    #[error("serialise config for write-back: {0}")]
    Serialize(#[source] serde_yml::Error),
    #[error("create parent dir {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("write config file {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("tighten permissions on {path}: {source}")]
    Permissions {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Serialise an overlay to YAML and write it to `path` atomically
/// with mode `0600`. If the parent directory is created here, it is
/// tightened to `0700`. Used by the setup-mode handler to persist the
/// newly-generated `StaticToken` entry so a restart does not loop
/// back into first-boot state.
///
/// **Atomicity** — writes to a sibling `.tmp` file, tightens its mode,
/// then renames. On the same filesystem the rename is atomic, so a
/// concurrent reader never sees a half-written config.
///
/// **Permissions** — on Unix, mode `0600` is enforced on the final
/// file and `0700` on any directory this function creates. On
/// non-Unix targets the mode bits are skipped silently; operators on
/// Windows are expected to ACL the parent directory themselves (see
/// SYSTEM-BOOTSTRAP.md § "Config write-back").
pub fn to_file(overlay: &AgentConfigOverlay, path: &Path) -> Result<(), WriteBackError> {
    let body = serde_yml::to_string(overlay).map_err(WriteBackError::Serialize)?;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|source| WriteBackError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
            tighten_dir(parent)?;
        }
    }

    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, body.as_bytes()).map_err(|source| WriteBackError::Write {
        path: tmp.clone(),
        source,
    })?;
    tighten_file(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|source| WriteBackError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(unix)]
fn tighten_file(path: &Path) -> Result<(), WriteBackError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|source| {
        WriteBackError::Permissions {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(unix)]
fn tighten_dir(path: &Path) -> Result<(), WriteBackError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|source| {
        WriteBackError::Permissions {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(not(unix))]
fn tighten_file(_path: &Path) -> Result<(), WriteBackError> {
    Ok(())
}

#[cfg(not(unix))]
fn tighten_dir(_path: &Path) -> Result<(), WriteBackError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Built from a YAML literal because `StaticTokenEntry`'s inner
    /// types (`Actor`, `ScopeSet`, `NodeId`) live in `spi` and aren't
    /// imported in this crate's source; the overlay round-trip is the
    /// actual target of the test.
    fn sample_overlay() -> AgentConfigOverlay {
        let yaml = r#"
auth:
  provider: static_token
  tokens:
    - token: "tok_abcdef"
      actor:
        kind: machine
        id: "00000000-0000-0000-0000-000000000001"
        label: "setup-admin"
      tenant: "default"
      scopes: [read_nodes, write_slots]
"#;
        serde_yml::from_str(yaml).expect("sample overlay parses")
    }

    #[test]
    fn to_file_round_trips_through_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.yaml");
        let overlay = sample_overlay();

        to_file(&overlay, &path).unwrap();
        let parsed = from_file(&path).unwrap();

        let before = serde_yml::to_string(&overlay).unwrap();
        let after = serde_yml::to_string(&parsed).unwrap();
        assert_eq!(before, after);
    }

    #[cfg(unix)]
    #[test]
    fn to_file_sets_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.yaml");
        to_file(&sample_overlay(), &path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn to_file_creates_nested_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("agent.yaml");
        to_file(&sample_overlay(), &path).unwrap();
        assert!(path.exists());
    }
}
