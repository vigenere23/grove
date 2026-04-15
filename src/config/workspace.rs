use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceConfigError {
    #[error("failed to read workspace config at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid workspace config at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("no .grove.toml found in {0}")]
    NotFound(PathBuf),
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub editor: Option<String>,
    #[serde(default)]
    pub copy: Vec<String>,
    #[serde(default)]
    pub link: Vec<String>,
    /// Shell commands to run after copy/link and after `--open` (if used). Workspace config only.
    #[serde(default)]
    pub post_create: Vec<String>,
}

pub struct ResolvedWorkspace {
    pub config: WorkspaceConfig,
    /// The directory containing the `.grove.toml` that was selected.
    pub config_dir: PathBuf,
}

const CONFIG_FILENAME: &str = ".grove.toml";

/// Load the workspace config from a specific directory.
/// Returns an error if `.grove.toml` does not exist there.
pub fn load_workspace_config(dir: &Path) -> Result<ResolvedWorkspace, WorkspaceConfigError> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let config_path = dir.join(CONFIG_FILENAME);

    if !config_path.is_file() {
        return Err(WorkspaceConfigError::NotFound(dir));
    }

    let contents =
        fs::read_to_string(&config_path).map_err(|e| WorkspaceConfigError::Read {
            path: config_path.clone(),
            source: e,
        })?;

    let config: WorkspaceConfig =
        toml::from_str(&contents).map_err(|e| WorkspaceConfigError::Parse {
            path: config_path,
            source: e,
        })?;

    Ok(ResolvedWorkspace {
        config,
        config_dir: dir,
    })
}

/// Walk from `start` up to (and including) `git_root`, looking for `.grove.toml`.
/// Returns the most specific (closest to `start`) config found.
pub fn find_workspace_config(
    start: &Path,
    git_root: &Path,
) -> Result<Option<ResolvedWorkspace>, WorkspaceConfigError> {
    let start = start
        .canonicalize()
        .unwrap_or_else(|_| start.to_path_buf());
    let git_root = git_root
        .canonicalize()
        .unwrap_or_else(|_| git_root.to_path_buf());

    let mut current = Some(start.as_path());

    while let Some(dir) = current {
        let config_path = dir.join(CONFIG_FILENAME);

        if config_path.is_file() {
            let contents =
                fs::read_to_string(&config_path).map_err(|e| WorkspaceConfigError::Read {
                    path: config_path.clone(),
                    source: e,
                })?;

            let config: WorkspaceConfig =
                toml::from_str(&contents).map_err(|e| WorkspaceConfigError::Parse {
                    path: config_path,
                    source: e,
                })?;

            return Ok(Some(ResolvedWorkspace {
                config,
                config_dir: dir.to_path_buf(),
            }));
        }

        if dir == git_root {
            break;
        }

        current = dir.parent();
    }

    Ok(None)
}
