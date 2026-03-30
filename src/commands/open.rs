use std::env;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::global::GlobalConfig;
use crate::config::state::{RepoState, WorktreeState};
use crate::config::workspace;
use crate::editor;
use crate::git;

pub fn run(id: String) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let mut state = RepoState::load(&git_root).context("failed to load state")?;

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

    let (editor_id, relative_workspace) = match state.get(&id) {
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
    };

    let editor_cfg = global_config.get_editor(&editor_id)?;
    let editor_workspace = worktree_info.path.join(&relative_workspace);

    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    println!("Editor opened at {}", editor_workspace.display());

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
