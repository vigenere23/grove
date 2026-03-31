use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("failed to read state file at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write state file at {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid state file at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to serialize state: {0}")]
    Serialize(#[from] toml::ser::Error),

    #[error("worktree '{0}' not found in state")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeState {
    pub editor: String,
    pub workspace: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoState {
    #[serde(default)]
    pub worktrees: BTreeMap<String, WorktreeState>,
}

const STATE_FILENAME: &str = ".state.toml";

impl RepoState {
    pub fn path(git_root: &Path) -> PathBuf {
        git::worktrees_base_dir(git_root).join(STATE_FILENAME)
    }

    pub fn load(git_root: &Path) -> Result<Self, StateError> {
        let path = Self::path(git_root);

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path).map_err(|e| StateError::Read {
            path: path.clone(),
            source: e,
        })?;

        toml::from_str(&contents).map_err(|e| StateError::Parse { path, source: e })
    }

    pub fn save(&self, git_root: &Path) -> Result<(), StateError> {
        let path = Self::path(git_root);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StateError::Write {
                path: path.clone(),
                source: e,
            })?;
        }

        let contents = toml::to_string_pretty(self)?;
        fs::write(&path, contents).map_err(|e| StateError::Write { path, source: e })?;

        Ok(())
    }

    pub fn add(&mut self, id: String, state: WorktreeState) {
        self.worktrees.insert(id, state);
    }

    pub fn get(&self, id: &str) -> Result<&WorktreeState, StateError> {
        self.worktrees
            .get(id)
            .ok_or_else(|| StateError::NotFound(id.to_string()))
    }

    pub fn remove(&mut self, id: &str) {
        self.worktrees.remove(id);
    }

    /// Remove state entries that don't correspond to any live git worktree.
    /// `worktree_paths` should be the paths reported by `git worktree list`.
    /// Returns the ids that were pruned.
    pub fn prune(&mut self, git_root: &Path, worktree_paths: &[PathBuf]) -> Vec<String> {
        let stale: Vec<String> = self
            .worktrees
            .keys()
            .filter(|id| {
                let expected = git::worktree_dir(git_root, id);
                !worktree_paths.contains(&expected)
            })
            .cloned()
            .collect();

        for id in &stale {
            self.worktrees.remove(id);
        }

        stale
    }
}
