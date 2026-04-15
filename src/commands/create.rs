use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::commands;
use crate::config::global::GlobalConfig;
use crate::config::state::{RepoState, WorktreeState};
use crate::config::workspace;
use crate::copy;
use crate::editor;
use crate::git;
use crate::output::*;
use crate::post_create;
use crate::prompt;

enum BranchAction {
    Create { from: String, name: String },
    Reuse { name: String },
}

fn merge_workspace_paths(into: &mut Vec<String>, extra: &[String]) {
    for path in extra {
        if !into.contains(path) {
            into.push(path.clone());
        }
    }
}

fn ensure_copy_and_link_disjoint(copy: &[String], link: &[String]) -> Result<()> {
    let copy_set: HashSet<_> = copy.iter().collect();
    let mut overlap: Vec<String> = link
        .iter()
        .filter(|p| copy_set.contains(p))
        .cloned()
        .collect();
    if overlap.is_empty() {
        return Ok(());
    }
    overlap.sort();
    overlap.dedup();
    bail!(
        "invalid config: the following path(s) appear in both `copy` and `link` after merging global, editor-specific, and workspace config: {}",
        overlap.join(", "),
    );
}

pub fn run(
    id: String,
    from: Option<String>,
    branch: Option<String>,
    workspace_override: Option<String>,
    open_editor: bool,
    editor_override: Option<String>,
) -> Result<()> {
    // ── Phase 1: gather context & validate ──────────────────────────────

    let git_root = git::repo_root().context("failed to find git repository root")?;
    verbose!("git root: {}", git_root.display());

    let global_config = GlobalConfig::load().context("failed to load global config")?;
    verbose!("global copy: {:?}", global_config.copy);
    verbose!("global link: {:?}", global_config.link);

    let cwd = env::current_dir().context("failed to determine current working directory")?;
    verbose!("cwd: {}", cwd.display());

    let (workspace_editor, workspace_dir, workspace_copy, workspace_link, workspace_post_create) =
        if let Some(ref ws) = workspace_override {
            let ws_path = cwd.join(ws);
            verbose!("workspace override: {}", ws_path.display());
            let resolved = workspace::load_workspace_config(&ws_path)
                .with_context(|| format!("failed to load workspace config from '{ws}'"))?;
            if let Some(ref editor_id) = resolved.config.editor {
                global_config.get_editor(editor_id)?;
            }
            let copy_inc = resolved.config.copy.clone();
            let link_inc = resolved.config.link.clone();
            let post_inc = resolved.config.post_create.clone();
            verbose!(
                "workspace config editor={:?}, copy={:?}, link={:?}, post_create={:?}, config_dir={}",
                resolved.config.editor,
                copy_inc,
                link_inc,
                post_inc,
                resolved.config_dir.display()
            );
            (
                resolved.config.editor,
                resolved.config_dir,
                copy_inc,
                link_inc,
                post_inc,
            )
        } else {
            match workspace::find_workspace_config(&cwd, &git_root)
                .context("failed to read workspace config")?
            {
                Some(resolved) => {
                    if let Some(ref editor_id) = resolved.config.editor {
                        global_config.get_editor(editor_id)?;
                    }
                    let copy_inc = resolved.config.copy.clone();
                    let link_inc = resolved.config.link.clone();
                    let post_inc = resolved.config.post_create.clone();
                    verbose!(
                        "workspace config found: editor={:?}, copy={:?}, link={:?}, post_create={:?}, config_dir={}",
                        resolved.config.editor,
                        copy_inc,
                        link_inc,
                        post_inc,
                        resolved.config_dir.display()
                    );
                    (
                        resolved.config.editor,
                        resolved.config_dir,
                        copy_inc,
                        link_inc,
                        post_inc,
                    )
                }
                None => {
                    bail!(
                        "no .grove.toml found between {} and git root {}\n\
                         Create one (optional `editor` = id from `grove config editors list`) or use -w/--workspace",
                        cwd.display(),
                        git_root.display(),
                    );
                }
            }
        };

    let relative_workspace = workspace_dir
        .strip_prefix(&git_root)
        .unwrap_or_else(|_| workspace_dir.as_ref());
    verbose!("relative_workspace: {}", relative_workspace.display());

    let copy_key = workspace_editor.as_deref().unwrap_or("");
    let mut copy_paths = global_config.resolved_copy(copy_key);
    verbose!("resolved copy (global+editor): {:?}", copy_paths);
    merge_workspace_paths(&mut copy_paths, &workspace_copy);
    verbose!("final copy paths: {:?}", copy_paths);

    let mut link_paths = global_config.resolved_link(copy_key);
    verbose!("resolved link (global+editor): {:?}", link_paths);
    merge_workspace_paths(&mut link_paths, &workspace_link);
    verbose!("final link paths: {:?}", link_paths);

    ensure_copy_and_link_disjoint(&copy_paths, &link_paths)?;

    if let Some(ref eid) = editor_override {
        global_config.get_editor(eid)?;
    }

    if open_editor && workspace_editor.is_none() && editor_override.is_none() {
        bail!(
            "cannot use --open: no `editor` in .grove.toml — set `editor` to an id from `grove config editors list`, or pass --editor <id>",
        );
    }

    let from_branch =
        from.map_or_else(|| git::current_branch().context("failed to get current branch"), Ok)?;

    let explicit_branch = branch.is_some();
    let desired_branch = branch.unwrap_or_else(|| format!("wip/{id}"));

    let (actual_id, worktree_path, branch_action) = resolve_conflicts(
        &git_root,
        &id,
        &from_branch,
        &desired_branch,
        explicit_branch,
        workspace_override.clone(),
        editor_override.clone(),
    )?;

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

    verbose!("workspace_editor: {:?}", workspace_editor);
    verbose!("workspace_dir: {}", workspace_dir.display());

    let editor_workspace = worktree_path.join(relative_workspace);
    verbose!("editor_workspace (dest): {}", editor_workspace.display());

    println!(
        "{} Worktree {} created on branch {} at {}",
        GREEN.paint("✔"),
        BOLD_CYAN.paint(&actual_id),
        BOLD_CYAN.paint(actual_branch),
        worktree_path.display(),
    );

    save_state(&git_root, &actual_id, relative_workspace)?;

    copy::copy_paths(&workspace_dir, &editor_workspace, &copy_paths);
    copy::link_paths(&workspace_dir, &editor_workspace, &link_paths);

    if open_editor {
        let Some(editor_id) = editor_override.as_deref().or(workspace_editor.as_deref()) else {
            bail!("internal error: --open without resolvable editor");
        };
        let editor_cfg = global_config.get_editor(editor_id)?;
        verbose!("opening editor '{}' with command: {}", editor_id, editor_cfg.command);
        editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;
    }

    if !workspace_post_create.is_empty() {
        post_create::run(&editor_workspace, &workspace_post_create)
            .context("post-create command")?;
    }

    if open_editor {
        println!(
            "{} Editor opened at {}",
            GREEN.paint("✔"),
            editor_workspace.display(),
        );
    }

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
    workspace_override: Option<String>,
    editor_override: Option<String>,
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
            let (actual_id, actual_path) = resolve_worktree_only(
                git_root,
                id,
                workspace_override.clone(),
                editor_override.clone(),
            )?;
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
            let (actual_id, actual_path, branch_action) = resolve_both(
                git_root,
                id,
                from_branch,
                desired_branch,
                explicit_branch,
                workspace_override.clone(),
                editor_override.clone(),
            )?;
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
fn resolve_worktree_only(
    git_root: &Path,
    id: &str,
    workspace_override: Option<String>,
    editor_override: Option<String>,
) -> Result<(String, PathBuf)> {
    match prompt::ask_worktree_conflict(id)? {
        prompt::WorktreeConflictChoice::OpenExisting => {
            commands::open::run(
                Some(id.to_string()),
                workspace_override,
                editor_override,
            )?;
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
    workspace_override: Option<String>,
    editor_override: Option<String>,
) -> Result<(String, PathBuf, BranchAction)> {
    match prompt::ask_both_conflict(id, desired_branch)? {
        prompt::BothConflictChoice::OpenExisting => {
            commands::open::run(
                Some(id.to_string()),
                workspace_override,
                editor_override,
            )?;
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

fn save_state(git_root: &Path, id: &str, workspace: &Path) -> Result<()> {
    let mut state = RepoState::load(git_root).context("failed to load state")?;
    state.add(
        id.to_string(),
        WorktreeState {
            workspace: workspace.to_path_buf(),
        },
    );
    state.save(git_root).context("failed to save state")?;
    Ok(())
}
