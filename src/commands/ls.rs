use std::path::Path;

use anyhow::{Context, Result};

use crate::config::state::RepoState;
use crate::git;
use crate::git::WorktreeInfo;
use crate::output::*;

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
        println!("{}", DIM.paint("No worktrees found."));
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

    for (i, wt) in non_main.iter().enumerate() {
        if i > 0 {
            println!();
        }

        let is_current = is_in_worktree
            && current_root.as_ref().is_some_and(|root| wt.path == *root);

        print_worktree(wt, is_current, &state, &git_root);
    }

    Ok(())
}

fn format_status(wt: &WorktreeInfo) -> String {
    if wt.is_bare {
        return String::new();
    }

    let mut parts = Vec::new();

    match git::is_dirty(&wt.path) {
        Ok(true) => parts.push(YELLOW.paint("uncommitted")),
        Ok(false) => {}
        Err(_) => parts.push(RED.paint("unknown")),
    }

    match git::ahead_behind(&wt.path) {
        Ok(ab) if ab.no_upstream => parts.push(YELLOW.paint("not pushed")),
        Ok(ab) => {
            if ab.ahead > 0 {
                parts.push(CYAN.paint(format!("↑{}", ab.ahead)));
            }
            if ab.behind > 0 {
                parts.push(CYAN.paint(format!("↓{}", ab.behind)));
            }
        }
        Err(_) => {}
    }

    if parts.is_empty() {
        format!(" {}", GREEN.paint("✔ clean"))
    } else {
        format!(" [{}]", parts.join(", "))
    }
}

fn print_worktree(wt: &WorktreeInfo, is_current: bool, state: &RepoState, git_root: &Path) {
    let branch_display = match &wt.branch {
        Some(b) => b.as_str(),
        None if wt.is_bare => "(bare)",
        None => "(detached)",
    };

    let status = format_status(wt);
    let current_marker = if is_current { "▸ " } else { "  " };

    let state_lookup = state
        .worktrees
        .iter()
        .find(|(id, _)| git::worktree_dir(git_root, id) == wt.path);

    let id_display = match state_lookup {
        Some((id, _)) => id.clone(),
        None => git::worktree_id(&wt.path).unwrap_or_else(|| "?".to_string()),
    };

    let workspace_display = state_lookup
        .map(|(_, ws)| format!("    workspace: {}", ws.workspace.display()));

    let tracked_marker = if state_lookup.is_none() {
        format!(" {}", DIM.paint("(untracked)"))
    } else {
        String::new()
    };

    let id_styled = if is_current {
        BOLD_CYAN.paint(&id_display)
    } else {
        BOLD.paint(&id_display)
    };

    println!("{current_marker}{id_styled}{status}{tracked_marker}");
    println!("    {}   {}", DIM.paint("path:"), wt.path.display());
    println!("    {} {}", DIM.paint("branch:"), CYAN.paint(branch_display));
    if let Some(ws) = workspace_display {
        println!("{ws}");
    }
}
