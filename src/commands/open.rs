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

pub fn run(id: Option<String>, workspace_override: Option<String>) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let mut state = RepoState::load(&git_root).context("failed to load state")?;

    let id = match id {
        Some(id) => id,
        None => select_worktree(&git_root)?,
    };

    let worktree_info = git::find_worktree(&git_root, &id)
        .context("failed to look up worktree")?
        .with_context(|| format!("no worktree found with id '{id}'"))?;

    if !worktree_info.path.exists() {
        bail!(
            "worktree directory does not exist: {}\nIt may have been removed or pruned.",
            worktree_info.path.display()
        );
    }

    let global_config = GlobalConfig::load().context("failed to load global config")?;

    let (editor_id, relative_workspace) = if let Some(ref ws) = workspace_override {
        let ws_path = git_root.join(ws);
        let resolved = workspace::load_workspace_config(&ws_path)
            .with_context(|| format!("failed to load workspace config from '{ws}'"))?;
        let editor_id = resolved.config.editor;
        global_config.get_editor(&editor_id)?;
        let rel_workspace = resolved
            .config_dir
            .strip_prefix(&git_root)
            .unwrap_or_else(|_| resolved.config_dir.as_ref())
            .to_path_buf();

        state.add(
            id.clone(),
            WorktreeState {
                editor: editor_id.clone(),
                workspace: rel_workspace.clone(),
            },
        );
        state.save(&git_root).context("failed to save state")?;

        (editor_id, rel_workspace)
    } else {
        match state.get(&id) {
            Ok(wt_state) => (wt_state.editor.clone(), wt_state.workspace.clone()),
            Err(_) => {
                let (editor_id, rel_workspace) =
                    resolve_editor(&global_config, &worktree_info.path)?;

                state.add(
                    id.clone(),
                    WorktreeState {
                        editor: editor_id.clone(),
                        workspace: rel_workspace.clone(),
                    },
                );
                state.save(&git_root).context("failed to save state")?;

                (editor_id, rel_workspace)
            }
        }
    };

    let editor_cfg = global_config.get_editor(&editor_id)?;
    let editor_workspace = worktree_info.path.join(&relative_workspace);

    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    println!("{} Editor opened for {} at {}", GREEN.paint("✔"), BOLD_CYAN.paint(&id), editor_workspace.display());

    Ok(())
}

fn resolve_editor(
    global_config: &GlobalConfig,
    worktree_path: &Path,
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
            let editor_id = resolved.config.editor;
            global_config.get_editor(&editor_id)?;
            let relative_workspace = resolved
                .config_dir
                .strip_prefix(worktree_path)
                .unwrap_or_else(|_| resolved.config_dir.as_ref())
                .to_path_buf();
            Ok((editor_id, relative_workspace))
        }
        None => {
            let (first_id, _) = global_config.first_editor()?;
            Ok((first_id.clone(), std::path::PathBuf::new()))
        }
    }
}

fn select_worktree(git_root: &Path) -> Result<String> {
    let worktrees = git::list_worktrees().context("failed to list worktrees")?;
    let state = RepoState::load(git_root).unwrap_or_default();

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
        bail!("no worktrees found — create one first with parai new");
    }

    prompt::select("Open worktree:", &items)
}
