use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::state::RepoState;
use crate::git;
use crate::output::*;
use crate::output::verbose;
use crate::prompt::{self, SelectItem};
use crate::BranchCleanup;

pub fn run(
    id: Option<String>,
    force: bool,
    all: bool,
    current: bool,
    branch_cleanup: Option<BranchCleanup>,
) -> Result<()> {
    if all {
        if id.is_some() || current {
            bail!("cannot combine --all with an id or --current");
        }
        return run_all(force, branch_cleanup);
    }

    if current {
        if id.is_some() {
            bail!("cannot specify both an id and --current");
        }
        return run_current(force, branch_cleanup);
    }

    let id = id.context("worktree id is required (or use --all / --current)")?;
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let worktree_paths: Vec<_> = worktrees.iter().map(|wt| wt.path.clone()).collect();

    let mut state = RepoState::load(&git_root).context("failed to load state")?;
    state.prune(&git_root, &worktree_paths);

    let branch_cleanup = resolve_branch_cleanup(branch_cleanup)?;
    remove_one(&id, &git_root, &mut state, force, &branch_cleanup)?;

    state.save(&git_root).context("failed to save state")?;
    Ok(())
}

fn run_current(force: bool, branch_cleanup: Option<BranchCleanup>) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let current_root =
        git::current_worktree_root().context("failed to determine current worktree root")?;

    if current_root == git_root {
        bail!("you are in the main worktree — --current can only be used inside a grove worktree");
    }

    let id = git::worktree_id(&current_root)
        .context("failed to derive worktree id from current directory")?;

    env::set_current_dir(&git_root).context("failed to change directory to repo root")?;

    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let worktree_paths: Vec<_> = worktrees.iter().map(|wt| wt.path.clone()).collect();

    let mut state = RepoState::load(&git_root).context("failed to load state")?;
    state.prune(&git_root, &worktree_paths);
    let branch_cleanup = resolve_branch_cleanup(branch_cleanup)?;
    remove_one(&id, &git_root, &mut state, force, &branch_cleanup)?;
    state.save(&git_root).context("failed to save state")?;

    Ok(())
}

fn resolve_branch_cleanup(explicit: Option<BranchCleanup>) -> Result<Option<BranchCleanup>> {
    if explicit.is_some() {
        return Ok(explicit);
    }

    let items = vec![
        SelectItem {
            id: "keep".into(),
            label: "Keep the branch".into(),
            key: Some('k'),
            ..Default::default()
        },
        SelectItem {
            id: "delete".into(),
            label: "Delete the local branch".into(),
            key: Some('d'),
            ..Default::default()
        },
        SelectItem {
            id: "delete-remote".into(),
            label: "Delete local and remote branches".into(),
            key: Some('r'),
            ..Default::default()
        },
    ];

    match prompt::select("Branch cleanup:", &items)?.as_str() {
        "delete" => Ok(Some(BranchCleanup::Delete)),
        "delete-remote" => Ok(Some(BranchCleanup::DeleteRemote)),
        _ => Ok(None),
    }
}

fn run_all(force: bool, branch_cleanup: Option<BranchCleanup>) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let worktree_paths: Vec<_> = worktrees.iter().map(|wt| wt.path.clone()).collect();

    let mut state = RepoState::load(&git_root).context("failed to load state")?;
    state.prune(&git_root, &worktree_paths);

    let candidates: Vec<(String, PathBuf)> = worktrees
        .iter()
        .filter(|wt| wt.path != git_root)
        .filter_map(|wt| git::worktree_id(&wt.path).map(|id| (id, wt.path.clone())))
        .collect();

    if candidates.is_empty() {
        println!("{}", DIM.paint("No worktrees to remove."));
        return Ok(());
    }

    let mut ids = Vec::new();
    let mut skipped = Vec::new();
    for (id, path) in &candidates {
        if !force {
            let dirty = git::is_dirty(path).unwrap_or(false);
            if dirty {
                skipped.push(id.as_str());
                continue;
            }
        }
        ids.push(id.as_str());
    }

    if ids.is_empty() && !skipped.is_empty() {
        println!("{}", DIM.paint("No clean worktrees to remove."));
        for id in &skipped {
            println!(
                "{} {}",
                BOLD.paint(*id),
                YELLOW.paint("skipped (dirty)")
            );
        }
        return Ok(());
    }

    let mut failed = 0usize;
    for id in &ids {
        if let Err(e) = remove_one(id, &git_root, &mut state, force, &branch_cleanup) {
            eprintln!("{} {} {e}", BOLD.paint(*id), RED.paint("✘"));
            failed += 1;
        }
    }

    for id in &skipped {
        println!(
            "{} {}",
            BOLD.paint(*id),
            YELLOW.paint("skipped (dirty)")
        );
    }

    state.save(&git_root).context("failed to save state")?;

    if failed > 0 {
        bail!("failed to remove {failed} worktree(s)");
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

    if !force && branch_cleanup.is_some() && (ab.no_upstream || ab.ahead > 0) {
        bail!(
            "worktree '{id}' has unpushed commits — push first, or use --force to delete the branch anyway"
        );
    }

    Ok(ab.ahead > 0)
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
    verbose!("removing worktree '{}' (force={})", id, force);
    let worktree_path = match resolve_worktree_path(id, git_root) {
        Ok(path) => path,
        Err(_) => {
            if state.get(id).is_ok() {
                state.remove(id);
                println!("{} {}", BOLD.paint(id), YELLOW.paint("worktree directory already gone, cleaned up state"));
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
    verbose!("worktree '{}' branch: {:?}, path: {}", id, branch, worktree_path.display());

    let has_unpushed = check_unpushed(id, &worktree_path, force, branch_cleanup)?;

    git::remove_worktree(&worktree_path, force).context("failed to remove worktree")?;

    if let Some(cleanup) = branch_cleanup {
        if let Some(ref branch_name) = branch {
            match cleanup {
                BranchCleanup::DeleteRemote => {
                    if let Err(e) = git::delete_remote_branch(branch_name) {
                        eprintln!("{} {} failed to delete remote branch: {e}", BOLD.paint(id), BOLD_YELLOW.paint("⚠"));
                    } else {
                        println!("{} deleted remote branch {}", BOLD.paint(id), CYAN.paint(branch_name));
                    }
                    if let Err(e) = git::delete_branch(branch_name) {
                        eprintln!("{} {} failed to delete local branch: {e}", BOLD.paint(id), BOLD_YELLOW.paint("⚠"));
                    } else {
                        println!("{} deleted local branch {}", BOLD.paint(id), CYAN.paint(branch_name));
                    }
                }
                BranchCleanup::Delete => {
                    if let Err(e) = git::delete_branch(branch_name) {
                        eprintln!("{} {} failed to delete local branch: {e}", BOLD.paint(id), BOLD_YELLOW.paint("⚠"));
                    } else {
                        println!("{} deleted local branch {}", BOLD.paint(id), CYAN.paint(branch_name));
                    }
                }
            }
        }
    } else if has_unpushed {
        eprintln!(
            "{} {} branch has unpushed commits (the branch is preserved locally)",
            BOLD.paint(id),
            BOLD_YELLOW.paint("⚠")
        );
    }

    state.remove(id);
    println!("{} {}", BOLD.paint(id), GREEN.paint("✔ removed"));

    Ok(())
}
