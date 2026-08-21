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

    #[error("workspace directory {0} is not inside the git repository or any of its worktrees")]
    WorkspaceOutsideRepo(PathBuf),

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
    parent.join(format!(".grove__{repo_name}"))
}

pub fn worktree_dir(git_root: &Path, id: &str) -> PathBuf {
    worktrees_base_dir(git_root).join(id)
}

/// Whether the checkout at `worktree_path` is the one a state entry keyed `id` refers to.
/// A worktree grove created lives at [`worktree_dir`]; one created directly with
/// `git worktree add ../foo` matches on its last path component instead. The main
/// checkout never matches by name — it is not a grove worktree.
///
/// This is the single definition of that rule: `RepoState::prune`, `commands::open` and
/// `commands::list` must agree, or state entries get pruned by one and looked up by
/// another.
pub fn worktree_matches_id(git_root: &Path, worktree_path: &Path, id: &str) -> bool {
    worktree_path == worktree_dir(git_root, id)
        || (worktree_path != git_root && worktree_id(worktree_path).as_deref() == Some(id))
}

/// Express `dir` relative to the checkout that contains it — the main checkout at
/// `git_root`, or any of `worktree_paths`. Both sides are canonicalized before
/// comparing, since `repo_root` is not canonical while workspace configs are, and the
/// longest matching prefix wins so nested checkouts resolve correctly. Hard-errors
/// rather than silently handing back an absolute path.
pub fn relative_workspace(
    git_root: &Path,
    worktree_paths: &[PathBuf],
    dir: &Path,
) -> Result<PathBuf, GitError> {
    // A path that does not exist yet degrades to itself rather than panicking.
    fn canonical(p: &Path) -> PathBuf {
        p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
    }

    let target = canonical(dir);
    let mut best: Option<(usize, PathBuf)> = None;

    for root in std::iter::once(git_root).chain(worktree_paths.iter().map(PathBuf::as_path)) {
        let root = canonical(root);
        if let Ok(rel) = target.strip_prefix(&root) {
            let depth = root.components().count();
            if best.as_ref().is_none_or(|(best_depth, _)| depth > *best_depth) {
                best = Some((depth, rel.to_path_buf()));
            }
        }
    }

    // The checkout root itself relativizes to an empty path, which is valid.
    best.map(|(_, rel)| rel)
        .ok_or(GitError::WorkspaceOutsideRepo(target))
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
        .args(["status", "--porcelain", "--no-untracked-files"])
        .current_dir(worktree_path)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(!output.stdout.is_empty())
}

#[derive(Debug, Default)]
pub struct AheadBehind {
    pub ahead: u32,
    pub behind: u32,
    /// true when the branch has no upstream configured (never pushed)
    pub no_upstream: bool,
}

pub fn ahead_behind(worktree_path: &Path) -> Result<AheadBehind, GitError> {
    let check = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(worktree_path)
        .output()
        .map_err(GitError::Io)?;

    if !check.status.success() {
        return Ok(AheadBehind {
            ahead: 0,
            behind: 0,
            no_upstream: true,
        });
    }

    let output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .current_dir(worktree_path)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\t').collect();
    if parts.len() != 2 {
        return Err(GitError::CommandFailed(format!(
            "unexpected rev-list output: {stdout}"
        )));
    }

    let behind = parts[0].parse::<u32>().unwrap_or(0);
    let ahead = parts[1].parse::<u32>().unwrap_or(0);

    Ok(AheadBehind {
        ahead,
        behind,
        no_upstream: false,
    })
}

pub fn branch_exists(name: &str) -> Result<bool, GitError> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{name}")])
        .output()
        .map_err(GitError::Io)?;

    Ok(output.status.success())
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

pub fn add_worktree_existing_branch(
    worktree_path: &Path,
    branch: &str,
) -> Result<(), GitError> {
    if worktree_path.exists() {
        return Err(GitError::WorktreeAlreadyExists(worktree_path.to_path_buf()));
    }

    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            &worktree_path.to_string_lossy(),
            branch,
        ])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(())
}

pub fn remove_worktree(worktree_path: &Path, force: bool) -> Result<(), GitError> {
    // Always use --force so `git worktree remove` doesn't refuse due to
    // untracked files (e.g. files from `copy` / `link`). The caller is
    // responsible for the uncommitted-changes safety check.
    let mut args = vec!["worktree", "remove", "--force"];
    if force {
        // Double --force: also removes worktrees with staged/unstaged changes.
        args.push("--force");
    }
    let path_str = worktree_path.to_string_lossy();
    args.push(&path_str);

    let output = Command::new("git")
        .args(&args)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(())
}

pub fn delete_branch(branch: &str) -> Result<(), GitError> {
    let output = Command::new("git")
        .args(["branch", "-D", branch])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(())
}

pub fn delete_remote_branch(branch: &str) -> Result<(), GitError> {
    let output = Command::new("git")
        .args(["push", "origin", "--delete", branch])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    Ok(())
}

/// Derive an id from a worktree path by taking the last path component.
pub fn worktree_id(worktree_path: &Path) -> Option<String> {
    worktree_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
}

/// Find a worktree by id. Checks the grove worktree dir first, then scans
/// all git worktrees for a directory whose name matches the id.
pub fn find_worktree(git_root: &Path, id: &str) -> Result<Option<WorktreeInfo>, GitError> {
    Ok(list_worktrees()?
        .into_iter()
        .find(|wt| worktree_matches_id(git_root, &wt.path, id)))
}

pub fn worktree_branch(worktree_path: &Path) -> Result<Option<String>, GitError> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::CommandFailed(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path_str));
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            current_branch = branch_ref.strip_prefix("refs/heads/").map(|s| s.to_string());
        } else if line == "detached" {
            current_branch = None;
        } else if line.is_empty() {
            if let Some(ref path) = current_path {
                if path == worktree_path {
                    return Ok(current_branch);
                }
            }
            current_path = None;
            current_branch = None;
        }
    }

    if let Some(ref path) = current_path {
        if path == worktree_path {
            return Ok(current_branch);
        }
    }

    Ok(None)
}
