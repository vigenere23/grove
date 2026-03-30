use std::path::Path;

use anyhow::{Context, Result};

use crate::config::state::RepoState;
use crate::git;
use crate::git::WorktreeInfo;

pub fn run() -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let state = RepoState::load(&git_root).unwrap_or_default();

    let current_root = git::current_worktree_root().ok();
    let is_in_worktree = current_root
        .as_ref()
        .is_some_and(|root| *root != git_root);

    let mut non_main: Vec<_> = worktrees
        .into_iter()
        .filter(|wt| wt.path != git_root)
        .collect();

    if non_main.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    non_main.reverse();

    if is_in_worktree {
        if let Some(pos) = current_root
            .as_ref()
            .and_then(|root| non_main.iter().position(|wt| wt.path == *root))
        {
            let current = non_main.remove(pos);
            non_main.insert(0, current);
        }
    }

    for wt in &non_main {
        let is_current = is_in_worktree
            && current_root.as_ref().is_some_and(|root| wt.path == *root);

        print_worktree(wt, is_current, &state, &git_root);
    }

    Ok(())
}

fn print_worktree(wt: &WorktreeInfo, is_current: bool, state: &RepoState, git_root: &Path) {
    let branch_display = match &wt.branch {
        Some(b) => b.as_str(),
        None if wt.is_bare => "(bare)",
        None => "(detached)",
    };

    let dirty_marker = if wt.is_bare {
        String::new()
    } else {
        match git::is_dirty(&wt.path) {
            Ok(true) => " [dirty]".to_string(),
            Ok(false) => " [clean]".to_string(),
            Err(_) => " [unknown]".to_string(),
        }
    };

    let current_marker = if is_current { "* " } else { "  " };

    let lookup = state
        .worktrees
        .iter()
        .find(|(id, _)| git::worktree_dir(git_root, id) == wt.path);

    let (id_display, workspace_display) = match lookup {
        Some((id, ws)) => (
            id.as_str().to_string(),
            format!("    workspace: {}", ws.workspace.display()),
        ),
        None => ("?".to_string(), String::new()),
    };

    println!("{current_marker}{id_display}{dirty_marker}");
    println!("    path:   {}", wt.path.display());
    println!("    branch: {branch_display}");
    if !workspace_display.is_empty() {
        println!("{workspace_display}");
    }
}
