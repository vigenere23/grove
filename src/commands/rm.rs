use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::state::RepoState;
use crate::git;
use crate::BranchCleanup;

pub fn run(
    id: Option<String>,
    force: bool,
    all: bool,
    branch_cleanup: Option<BranchCleanup>,
) -> Result<()> {
    if all {
        if id.is_some() {
            bail!("cannot specify both an id and --all");
        }
        return run_all(force, branch_cleanup);
    }

    let id = id.context("worktree id is required (or use --all)")?;
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let mut state = RepoState::load(&git_root).context("failed to load state")?;

    remove_one(&id, &git_root, &mut state, force, &branch_cleanup)?;

    state.save(&git_root).context("failed to save state")?;
    Ok(())
}

fn run_all(force: bool, branch_cleanup: Option<BranchCleanup>) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let mut state = RepoState::load(&git_root).context("failed to load state")?;

    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let ids: Vec<String> = worktrees
        .iter()
        .filter(|wt| wt.path != git_root)
        .filter_map(|wt| git::worktree_id(&wt.path))
        .collect();

    if ids.is_empty() {
        println!("No worktrees to remove.");
        return Ok(());
    }

    let mut errors = Vec::new();
    for id in &ids {
        if let Err(e) = remove_one(id, &git_root, &mut state, force, &branch_cleanup) {
            errors.push(format!("{id}: {e}"));
        }
    }

    state.save(&git_root).context("failed to save state")?;

    if !errors.is_empty() {
        bail!(
            "failed to remove some worktrees:\n{}",
            errors.join("\n")
        );
    }

    Ok(())
}

/// Returns true if the worktree has unpushed commits.
/// Blocks `--branch delete` on unpushed commits unless `--force` is set.
fn check_unpushed(
    id: &str,
    worktree_path: &Path,
    force: bool,
    branch_cleanup: &Option<BranchCleanup>,
) -> Result<bool> {
    let ab = match git::ahead_behind(worktree_path) {
        Ok(ab) => ab,
        Err(_) => return Ok(false),
    };

    let has_unpushed = ab.no_upstream || ab.ahead > 0;

    if has_unpushed && !force && branch_cleanup.is_some() {
        bail!(
            "worktree '{id}' has unpushed commits — push first, or use --force to delete the branch anyway"
        );
    }

    Ok(has_unpushed)
}

fn resolve_worktree_path(id: &str, git_root: &Path) -> Result<PathBuf> {
    match git::find_worktree(git_root, id).context("failed to look up worktree")? {
        Some(wt) => Ok(wt.path),
        None => bail!("no worktree found with id '{id}'"),
    }
}

fn remove_one(
    id: &str,
    git_root: &Path,
    state: &mut RepoState,
    force: bool,
    branch_cleanup: &Option<BranchCleanup>,
) -> Result<()> {
    let worktree_path = match resolve_worktree_path(id, git_root) {
        Ok(path) => path,
        Err(_) => {
            if state.get(id).is_ok() {
                state.remove(id);
                println!("{id}: worktree directory already gone, cleaned up state");
                return Ok(());
            }
            bail!("no worktree found with id '{id}'");
        }
    };

    if !force {
        let dirty = git::is_dirty(&worktree_path).context("failed to check worktree status")?;
        if dirty {
            bail!(
                "worktree '{id}' has uncommitted changes — commit/stash them first, or use --force"
            );
        }
    }

    let branch = git::worktree_branch(&worktree_path)
        .context("failed to determine worktree branch")?;

    let has_unpushed = check_unpushed(id, &worktree_path, force, branch_cleanup)?;

    git::remove_worktree(&worktree_path, force).context("failed to remove worktree")?;

    if let Some(cleanup) = branch_cleanup {
        if let Some(ref branch_name) = branch {
            match cleanup {
                BranchCleanup::DeleteRemote => {
                    if let Err(e) = git::delete_remote_branch(branch_name) {
                        eprintln!("{id}: warning: failed to delete remote branch: {e}");
                    } else {
                        println!("{id}: deleted remote branch '{branch_name}'");
                    }
                    if let Err(e) = git::delete_branch(branch_name) {
                        eprintln!("{id}: warning: failed to delete local branch: {e}");
                    } else {
                        println!("{id}: deleted local branch '{branch_name}'");
                    }
                }
                BranchCleanup::Delete => {
                    if let Err(e) = git::delete_branch(branch_name) {
                        eprintln!("{id}: warning: failed to delete local branch: {e}");
                    } else {
                        println!("{id}: deleted local branch '{branch_name}'");
                    }
                }
            }
        }
    } else if has_unpushed {
        eprintln!(
            "{id}: warning: branch has unpushed commits (the branch is preserved locally)"
        );
    }

    state.remove(id);
    println!("{id}: removed");

    Ok(())
}
