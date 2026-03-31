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
    verbose!("git root: {}", git_root.display());

    let global_config = GlobalConfig::load().context("failed to load global config")?;
    verbose!("global include: {:?}", global_config.include);

    let (editor_id, workspace_dir, workspace_include) =
        if let Some(ref ws) = workspace_override {
            let ws_path = git_root.join(ws);
            verbose!("workspace override: {}", ws_path.display());
            let resolved = workspace::load_workspace_config(&ws_path)
                .with_context(|| format!("failed to load workspace config from '{ws}'"))?;
            let editor_id = resolved.config.editor;
            global_config.get_editor(&editor_id)?;
            let inc = resolved.config.include;
            verbose!("workspace config editor={}, include={:?}, config_dir={}", editor_id, inc, resolved.config_dir.display());
            (editor_id, resolved.config_dir, inc)
        } else {
            let cwd =
                env::current_dir().context("failed to determine current working directory")?;
            verbose!("cwd: {}", cwd.display());
            match workspace::find_workspace_config(&cwd, &git_root)
                .context("failed to read workspace config")?
            {
                Some(resolved) => {
                    let editor_id = resolved.config.editor;
                    global_config.get_editor(&editor_id)?;
                    let inc = resolved.config.include;
                    verbose!("workspace config found: editor={}, include={:?}, config_dir={}", editor_id, inc, resolved.config_dir.display());
                    (editor_id, resolved.config_dir, inc)
                }
                None => {
                    let (first_id, _) = global_config.first_editor()?;
                    verbose!("no workspace config found, using first editor: {}", first_id);
                    (first_id.clone(), git_root.clone(), Vec::new())
                }
            }
        };

    let from_branch =
        from.map_or_else(|| git::current_branch().context("failed to get current branch"), Ok)?;

    let explicit_branch = branch.is_some();
    let desired_branch = branch.unwrap_or_else(|| format!("wip/{id}"));

    let (actual_id, worktree_path, branch_action) =
        resolve_conflicts(&git_root, &id, &from_branch, &desired_branch, explicit_branch)?;

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

    let relative_workspace = workspace_dir
        .strip_prefix(&git_root)
        .unwrap_or_else(|_| workspace_dir.as_ref());
    verbose!("editor_id: {}", editor_id);
    verbose!("workspace_dir: {}", workspace_dir.display());
    verbose!("relative_workspace: {}", relative_workspace.display());

    let editor_workspace = worktree_path.join(relative_workspace);
    verbose!("editor_workspace (dest): {}", editor_workspace.display());

    let mut include_paths = global_config.resolved_include(&editor_id);
    verbose!("resolved include (global+editor): {:?}", include_paths);
    for path in workspace_include {
        if !include_paths.contains(&path) {
            include_paths.push(path);
        }
    }
    verbose!("final include paths: {:?}", include_paths);

    let copied = include::copy_includes(&workspace_dir, &editor_workspace, &include_paths);

    let editor_cfg = global_config.get_editor(&editor_id)?;
    verbose!("opening editor '{}' with command: {}", editor_id, editor_cfg.command);
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

/// Check both worktree and branch existence upfront, then present the
/// appropriate prompt for the actual situation. Returns the final
/// (id, worktree_path, branch_action).
fn resolve_conflicts(
    git_root: &Path,
    id: &str,
    from_branch: &str,
    desired_branch: &str,
    explicit_branch: bool,
) -> Result<(String, PathBuf, BranchAction)> {
    let worktree_path = git::worktree_dir(git_root, id);
    let wt_exists = worktree_path.exists();
    let branch_exists =
        git::branch_exists(desired_branch).context("failed to check if branch exists")?;

    match (wt_exists, branch_exists) {
        // ── Nothing conflicts ───────────────────────────────────────
        (false, false) => Ok((
            id.to_string(),
            worktree_path,
            BranchAction::Create {
                from: from_branch.to_string(),
                name: desired_branch.to_string(),
            },
        )),

        // ── Only the branch exists ──────────────────────────────────
        (false, true) => {
            let branch_action =
                resolve_branch_only(id, from_branch, desired_branch, explicit_branch)?;
            Ok((id.to_string(), worktree_path, branch_action))
        }

        // ── Only the worktree exists ────────────────────────────────
        (true, false) => {
            let (actual_id, actual_path) =
                resolve_worktree_only(git_root, id)?;
            Ok((
                actual_id,
                actual_path,
                BranchAction::Create {
                    from: from_branch.to_string(),
                    name: desired_branch.to_string(),
                },
            ))
        }

        // ── Both exist ──────────────────────────────────────────────
        (true, true) => {
            let (actual_id, actual_path, branch_action) =
                resolve_both(git_root, id, from_branch, desired_branch, explicit_branch)?;
            Ok((actual_id, actual_path, branch_action))
        }
    }
}

/// Only the branch exists — ask whether to reuse it or pick a new name.
fn resolve_branch_only(
    id: &str,
    from_branch: &str,
    desired_branch: &str,
    explicit_branch: bool,
) -> Result<BranchAction> {
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

    match prompt::ask_branch_conflict(desired_branch)? {
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

/// Only the worktree directory exists — ask whether to open it or pick a new name.
fn resolve_worktree_only(git_root: &Path, id: &str) -> Result<(String, PathBuf)> {
    match prompt::ask_worktree_conflict(id)? {
        prompt::WorktreeConflictChoice::OpenExisting => {
            commands::open::run(Some(id.to_string()), None)?;
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

/// Both the worktree and branch exist — handle the combined conflict.
fn resolve_both(
    git_root: &Path,
    id: &str,
    from_branch: &str,
    desired_branch: &str,
    explicit_branch: bool,
) -> Result<(String, PathBuf, BranchAction)> {
    match prompt::ask_both_conflict(id, desired_branch)? {
        prompt::BothConflictChoice::OpenExisting => {
            commands::open::run(Some(id.to_string()), None)?;
            std::process::exit(0);
        }
        prompt::BothConflictChoice::NewName(new_id) => {
            let new_path = git::worktree_dir(git_root, &new_id);
            if new_path.exists() {
                bail!(
                    "worktree '{}' also already exists — please pick a unique name",
                    BOLD_CYAN.paint(&new_id),
                );
            }
            let branch_action = if explicit_branch {
                BranchAction::Reuse {
                    name: desired_branch.to_string(),
                }
            } else {
                let derived = format!("wip/{new_id}");
                if git::branch_exists(&derived).context("failed to check if branch exists")? {
                    resolve_branch_only(&new_id, from_branch, &derived, false)?
                } else {
                    BranchAction::Create {
                        from: from_branch.to_string(),
                        name: derived,
                    }
                }
            };
            Ok((new_id, new_path, branch_action))
        }
        prompt::BothConflictChoice::Abort => bail!("aborted"),
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
