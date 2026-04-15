use std::env;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::global::GlobalConfig;
use crate::config::state::{RepoState, WorktreeState};
use crate::config::workspace;
use crate::editor;
use crate::git;
use crate::output::*;
use crate::prompt::{self, SelectItem};

pub fn run(
    id: Option<String>,
    workspace_override: Option<String>,
    editor_override: Option<String>,
) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    verbose!("git root: {}", git_root.display());
    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let worktree_paths: Vec<_> = worktrees.iter().map(|wt| wt.path.clone()).collect();

    let mut state = RepoState::load(&git_root).context("failed to load state")?;
    let pruned = state.prune(&git_root, &worktree_paths);
    if !pruned.is_empty() {
        verbose!("pruned stale worktrees: {:?}", pruned);
        state.save(&git_root).context("failed to save state")?;
    }

    let id = match id {
        Some(id) => id,
        None => select_worktree(&git_root, &worktrees, &state)?,
    };
    verbose!("opening worktree: {}", id);

    let worktree_info = worktrees
        .iter()
        .find(|wt| {
            wt.path == git::worktree_dir(&git_root, &id)
                || (wt.path != git_root && git::worktree_id(&wt.path).as_deref() == Some(&id))
        })
        .with_context(|| format!("no worktree found with id '{id}'"))?;

    let global_config = GlobalConfig::load().context("failed to load global config")?;
    let ov = editor_override.as_deref();

    let (editor_id, relative_workspace) = if let Some(ref ws) = workspace_override {
        let cwd = env::current_dir().context("failed to determine current working directory")?;
        let ws_path = cwd.join(ws);
        let resolved = workspace::load_workspace_config(&ws_path)
            .with_context(|| format!("failed to load workspace config from '{ws}'"))?;
        let editor_id = match (&resolved.config.editor, ov) {
            (_, Some(o)) => {
                global_config.get_editor(o)?;
                o.to_string()
            }
            (Some(id), None) => {
                global_config.get_editor(id)?;
                id.clone()
            }
            (None, None) => bail!(
                "no `editor` in .grove.toml under '{ws}' — set `editor` to an id from `grove config editors list`, or pass --editor <id>",
            ),
        };
        let rel_workspace = resolved
            .config_dir
            .strip_prefix(&git_root)
            .unwrap_or_else(|_| resolved.config_dir.as_ref())
            .to_path_buf();

        state.add(
            id.clone(),
            WorktreeState {
                workspace: rel_workspace.clone(),
            },
        );
        state.save(&git_root).context("failed to save state")?;

        (editor_id, rel_workspace)
    } else {
        match state.get(&id) {
            Ok(wt_state) => {
                let relative_workspace = wt_state.workspace.clone();
                let editor_id = match ov {
                    Some(o) => {
                        global_config.get_editor(o)?;
                        o.to_string()
                    }
                    None => {
                        let source_ws = git_root.join(&relative_workspace);
                        let resolved = workspace::load_workspace_config(&source_ws)
                            .context("failed to load workspace config from source repo")?;
                        match resolved.config.editor {
                            Some(ref id) => {
                                global_config.get_editor(id)?;
                                id.clone()
                            }
                            None => bail!(
                                "no `editor` in .grove.toml — set `editor` to an id from `grove config editors list`, or pass --editor <id>",
                            ),
                        }
                    }
                };
                (editor_id, relative_workspace)
            }
            Err(_) => {
                let (editor_id, rel_workspace) =
                    resolve_editor(&global_config, &worktree_info.path, ov)?;

                state.add(
                    id.clone(),
                    WorktreeState {
                        workspace: rel_workspace.clone(),
                    },
                );
                state.save(&git_root).context("failed to save state")?;

                (editor_id, rel_workspace)
            }
        }
    };

    verbose!("editor_id: {}, relative_workspace: {}", editor_id, relative_workspace.display());

    let editor_cfg = global_config.get_editor(&editor_id)?;
    let editor_workspace = worktree_info.path.join(&relative_workspace);
    verbose!("editor_workspace: {}", editor_workspace.display());

    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    println!("{} Editor opened for {} at {}", GREEN.paint("✔"), BOLD_CYAN.paint(&id), editor_workspace.display());

    Ok(())
}

fn resolve_editor(
    global_config: &GlobalConfig,
    worktree_path: &Path,
    editor_override: Option<&str>,
) -> Result<(String, std::path::PathBuf)> {
    let cwd = env::current_dir().context("failed to determine current working directory")?;

    let search_dir = if cwd.starts_with(worktree_path) {
        cwd.clone()
    } else {
        worktree_path.to_path_buf()
    };

    match workspace::find_workspace_config(&search_dir, worktree_path)
        .context("failed to read workspace config")?
    {
        Some(resolved) => {
            let relative_workspace = resolved
                .config_dir
                .strip_prefix(worktree_path)
                .unwrap_or_else(|_| resolved.config_dir.as_ref())
                .to_path_buf();
            let editor_id = match (&resolved.config.editor, editor_override) {
                (_, Some(o)) => {
                    global_config.get_editor(o)?;
                    o.to_string()
                }
                (Some(id), None) => {
                    global_config.get_editor(id)?;
                    id.clone()
                }
                (None, None) => bail!(
                    "no `editor` in .grove.toml under this worktree — set `editor` to an id from `grove config editors list`, or pass --editor <id>",
                ),
            };
            Ok((editor_id, relative_workspace))
        }
        None => bail!(
            "no .grove.toml found between {} and worktree root {} — add one with an `editor` key, or use --workspace",
            search_dir.display(),
            worktree_path.display(),
        ),
    }
}

fn select_worktree(
    git_root: &Path,
    worktrees: &[git::WorktreeInfo],
    state: &RepoState,
) -> Result<String> {
    let current_root = git::current_worktree_root().ok();
    let is_in_worktree = current_root
        .as_ref()
        .is_some_and(|root| *root != *git_root);

    let items: Vec<SelectItem> = worktrees
        .iter()
        .filter(|wt| wt.path != *git_root)
        .map(|wt| {
            let id = state
                .worktrees
                .iter()
                .find(|(id, _)| git::worktree_dir(git_root, id) == wt.path)
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| {
                    git::worktree_id(&wt.path).unwrap_or_else(|| "?".to_string())
                });

            let branch = wt
                .branch
                .as_deref()
                .unwrap_or(if wt.is_bare { "(bare)" } else { "(detached)" });

            let is_current = is_in_worktree
                && current_root.as_ref().is_some_and(|root| wt.path == *root);

            let label = if is_current {
                format!("{id}  ({branch})  (current)")
            } else {
                format!("{id}  ({branch})")
            };

            SelectItem {
                label,
                id,
                disabled: is_current,
                ..Default::default()
            }
        })
        .collect();

    if items.is_empty() {
        bail!("no worktrees found — create one first with grove new");
    }

    prompt::select("Open worktree:", &items)
}
