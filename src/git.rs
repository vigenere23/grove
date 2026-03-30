use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("not inside a git repository (or any parent up to the filesystem root)")]
    NotARepo,

    #[error("could not determine the current branch — HEAD may be detached")]
    DetachedHead,

    #[error("worktree already exists at {0}")]
    WorktreeAlreadyExists(PathBuf),

    #[error("git command failed: {0}")]
    CommandFailed(String),

    #[error("failed to execute git: {0}")]
    Io(#[from] std::io::Error),
}

pub fn current_worktree_root() -> Result<PathBuf, GitError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        return Err(GitError::NotARepo);
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

pub fn repo_root() -> Result<PathBuf, GitError> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        return Err(GitError::NotARepo);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .ok_or(GitError::NotARepo)
}

pub fn current_branch() -> Result<String, GitError> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        return Err(GitError::DetachedHead);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Err(GitError::DetachedHead);
    }

    Ok(branch)
}

pub fn worktrees_base_dir(git_root: &Path) -> PathBuf {
    let repo_name = git_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    let parent = git_root.parent().unwrap_or(git_root);
    parent.join(format!(".{repo_name}_worktrees"))
}

pub fn worktree_dir(git_root: &Path, id: &str) -> PathBuf {
    worktrees_base_dir(git_root).join(id)
}

pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_bare: bool,
}

pub fn list_worktrees() -> Result<Vec<WorktreeInfo>, GitError> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut is_bare = false;

    for line in stdout.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path_str));
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            current_branch = branch_ref.strip_prefix("refs/heads/").map(|s| s.to_string());
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            current_branch = None;
        } else if line.is_empty() {
            if let Some(path) = current_path.take() {
                worktrees.push(WorktreeInfo {
                    path,
                    branch: current_branch.take(),
                    is_bare,
                });
            }
            is_bare = false;
        }
    }

    if let Some(path) = current_path.take() {
        worktrees.push(WorktreeInfo {
            path,
            branch: current_branch.take(),
            is_bare,
        });
    }

    Ok(worktrees)
}

pub fn is_dirty(worktree_path: &Path) -> Result<bool, GitError> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(!output.stdout.is_empty())
}

pub fn add_worktree(
    worktree_path: &Path,
    from_branch: &str,
    new_branch: &str,
) -> Result<(), GitError> {
    if worktree_path.exists() {
        return Err(GitError::WorktreeAlreadyExists(worktree_path.to_path_buf()));
    }

    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            new_branch,
            &worktree_path.to_string_lossy(),
            from_branch,
        ])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(())
}
