use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::commands;
use crate::config::global::GlobalConfig;
use crate::config::state::{RepoState, WorktreeState};
use crate::config::workspace;
use crate::editor;
use crate::git;
use crate::include;
use crate::output::*;
use crate::prompt;

enum BranchAction {
    Create { from: String, name: String },
    Reuse { name: String },
}

pub fn run(
    id: String,
    from: Option<String>,
    branch: Option<String>,
    workspace_override: Option<String>,
) -> Result<()> {
    // ── Phase 1: gather context & validate ──────────────────────────────

    let git_root = git::repo_root().context("failed to find git repository root")?;

    let global_config = GlobalConfig::load().context("failed to load global config")?;

    let (editor_id, workspace_dir, workspace_include) =
        if let Some(ref ws) = workspace_override {
            let ws_path = git_root.join(ws);
            let resolved = workspace::load_workspace_config(&ws_path)
                .with_context(|| format!("failed to load workspace config from '{ws}'"))?;
            let editor_id = resolved.config.editor;
            global_config.get_editor(&editor_id)?;
            let inc = resolved.config.include;
            (editor_id, resolved.config_dir, inc)
        } else {
            let cwd =
                env::current_dir().context("failed to determine current working directory")?;
            match workspace::find_workspace_config(&cwd, &git_root)
                .context("failed to read workspace config")?
            {
                Some(resolved) => {
                    let editor_id = resolved.config.editor;
                    global_config.get_editor(&editor_id)?;
                    let inc = resolved.config.include;
                    (editor_id, resolved.config_dir, inc)
                }
                None => {
                    let (first_id, _) = global_config.first_editor()?;
                    (first_id.clone(), git_root.clone(), Vec::new())
                }
            }
        };

    let from_branch =
        from.map_or_else(|| git::current_branch().context("failed to get current branch"), Ok)?;

    let explicit_branch = branch.is_some();
    let desired_branch = branch.unwrap_or_else(|| format!("wip/{id}"));

    let (actual_id, worktree_path) =
        resolve_worktree_id(&git_root, &id, &desired_branch)?;

    let branch_action = resolve_branch(&actual_id, &from_branch, &desired_branch, explicit_branch)?;

    // ── Phase 2: execute actions ────────────────────────────────────────

    let actual_branch = match &branch_action {
        BranchAction::Create { from, name } => {
            git::add_worktree(&worktree_path, from, name)
                .context("failed to create git worktree")?;
            name.as_str()
        }
        BranchAction::Reuse { name } => {
            git::add_worktree_existing_branch(&worktree_path, name)
                .context("failed to create git worktree")?;
            name.as_str()
        }
    };

    let mut include_paths = global_config.resolved_include(&editor_id);
    for path in workspace_include {
        if !include_paths.contains(&path) {
            include_paths.push(path);
        }
    }

    let copied = include::copy_includes(&git_root, &worktree_path, &include_paths);

    let relative_workspace = workspace_dir
        .strip_prefix(&git_root)
        .unwrap_or_else(|_| workspace_dir.as_ref());

    let editor_workspace = worktree_path.join(relative_workspace);

    let editor_cfg = global_config.get_editor(&editor_id)?;
    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    save_state(&git_root, &actual_id, &editor_id, relative_workspace)?;

    println!(
        "{} Worktree {} created on branch {} at {}",
        GREEN.paint("✔"),
        BOLD_CYAN.paint(&actual_id),
        BOLD_CYAN.paint(actual_branch),
        worktree_path.display(),
    );
    for path in &copied {
        println!("  {} {}", DIM.paint("copied"), path);
    }
    println!(
        "{} Editor opened at {}",
        GREEN.paint("✔"),
        editor_workspace.display(),
    );

    Ok(())
}

/// Resolve the worktree id/path, prompting the user if a worktree with that
/// name already exists. Returns the final (id, path) to use.
fn resolve_worktree_id(git_root: &Path, id: &str, branch: &str) -> Result<(String, PathBuf)> {
    let worktree_path = git::worktree_dir(git_root, id);

    if !worktree_path.exists() {
        return Ok((id.to_string(), worktree_path));
    }

    match prompt::ask_worktree_conflict(id, branch)? {
        prompt::WorktreeConflictChoice::OpenExisting => {
            commands::open::run(id.to_string(), None)?;
            std::process::exit(0);
        }
        prompt::WorktreeConflictChoice::NewName(new_id) => {
            let new_path = git::worktree_dir(git_root, &new_id);
            if new_path.exists() {
                bail!(
                    "worktree '{}' also already exists — please pick a unique name",
                    BOLD_CYAN.paint(&new_id),
                );
            }
            Ok((new_id, new_path))
        }
        prompt::WorktreeConflictChoice::Abort => bail!("aborted"),
    }
}

fn resolve_branch(
    id: &str,
    from_branch: &str,
    desired_branch: &str,
    explicit_branch: bool,
) -> Result<BranchAction> {
    if !git::branch_exists(desired_branch).context("failed to check if branch exists")? {
        return Ok(BranchAction::Create {
            from: from_branch.to_string(),
            name: desired_branch.to_string(),
        });
    }

    // When the user picked a new worktree name via the conflict prompt and
    // the branch was auto-derived from the original id, re-derive it from
    // the new id to avoid a stale conflict.
    if !explicit_branch {
        let derived = format!("wip/{id}");
        if derived != desired_branch
            && !git::branch_exists(&derived).context("failed to check if branch exists")?
        {
            return Ok(BranchAction::Create {
                from: from_branch.to_string(),
                name: derived,
            });
        }
    }

    match prompt::ask_branch_conflict(id, desired_branch)? {
        prompt::BranchConflictChoice::Reuse => Ok(BranchAction::Reuse {
            name: desired_branch.to_string(),
        }),
        prompt::BranchConflictChoice::NewName(new_name) => {
            if git::branch_exists(&new_name).context("failed to check if branch exists")? {
                bail!(
                    "branch '{}' also already exists — please pick a unique name",
                    BOLD_CYAN.paint(&new_name),
                );
            }
            Ok(BranchAction::Create {
                from: from_branch.to_string(),
                name: new_name,
            })
        }
        prompt::BranchConflictChoice::Abort => bail!("aborted"),
    }
}

fn save_state(git_root: &Path, id: &str, editor_id: &str, workspace: &Path) -> Result<()> {
    let mut state = RepoState::load(git_root).context("failed to load state")?;
    state.add(
        id.to_string(),
        WorktreeState {
            editor: editor_id.to_string(),
            workspace: workspace.to_path_buf(),
        },
    );
    state.save(git_root).context("failed to save state")?;
    Ok(())
}
