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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorktreeState {
    /// Legacy grove <=0.1 single-workspace field. Read for migration, never written.
    /// `skip_serializing_if` is load-bearing: the `toml` crate errors on `Option::None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<PathBuf>,

    /// Workspaces provisioned in this worktree, kept sorted. Membership means grove has
    /// already run copy/link/post_create for this (worktree, workspace) pair.
    #[serde(default)]
    workspaces: Vec<PathBuf>,
}

impl WorktreeState {
    pub fn new(ws: &Path) -> Self {
        Self {
            workspace: None,
            workspaces: vec![ws.to_path_buf()],
        }
    }

    pub fn workspaces(&self) -> &[PathBuf] {
        &self.workspaces
    }

    pub fn contains(&self, ws: &Path) -> bool {
        self.workspaces.iter().any(|w| w == ws)
    }

    /// Sorted insert. Returns true only when `ws` was not already recorded.
    pub fn insert(&mut self, ws: &Path) -> bool {
        match self.workspaces.binary_search_by(|w| w.as_path().cmp(ws)) {
            Ok(_) => false,
            Err(pos) => {
                self.workspaces.insert(pos, ws.to_path_buf());
                true
            }
        }
    }

    /// Fold the legacy `workspace` field into `workspaces`. Idempotent.
    pub fn migrate(&mut self) {
        // Normalize first: `insert` assumes a sorted list, and neither a legacy nor a
        // hand-edited state file offers that guarantee.
        self.workspaces.sort();
        self.workspaces.dedup();

        if let Some(legacy) = self.workspace.take() {
            self.insert(&legacy);
        }
    }
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

        let mut state: Self =
            toml::from_str(&contents).map_err(|e| StateError::Parse { path, source: e })?;

        for entry in state.worktrees.values_mut() {
            entry.migrate();
        }

        Ok(state)
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

    /// Record `ws` as provisioned for `id`, creating the entry if it does not exist.
    /// Returns true only when the workspace was newly added.
    pub fn insert_workspace(&mut self, id: &str, ws: &Path) -> bool {
        self.worktrees.entry(id.to_string()).or_default().insert(ws)
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
                !worktree_paths
                    .iter()
                    .any(|p| git::worktree_matches_id(git_root, p, id))
            })
            .cloned()
            .collect();

        for id in &stale {
            self.worktrees.remove(id);
        }

        stale
    }
}
