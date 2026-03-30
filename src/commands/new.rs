use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::global::GlobalConfig;
use crate::config::state::{RepoState, WorktreeState};
use crate::config::workspace;
use crate::editor;
use crate::git;

pub fn run(id: String, from: Option<String>, branch: Option<String>) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;

    let cwd = env::current_dir().context("failed to determine current working directory")?;

    let global_config = GlobalConfig::load().context("failed to load global config")?;

    let (editor_id, workspace_dir) =
        match workspace::find_workspace_config(&cwd, &git_root)
            .context("failed to read workspace config")?
        {
            Some(resolved) => {
                let editor_id = resolved.config.editor;
                global_config.get_editor(&editor_id)?;
                (editor_id, resolved.config_dir)
            }
            None => {
                let (first_id, _) = global_config.first_editor()?;
                (first_id.clone(), git_root.clone())
            }
        };

    let from_branch =
        from.map_or_else(|| git::current_branch().context("failed to get current branch"), Ok)?;

    let new_branch = branch.unwrap_or_else(|| format!("wip/{id}"));

    let worktree_path = git::worktree_dir(&git_root, &id);

    git::add_worktree(&worktree_path, &from_branch, &new_branch)
        .context("failed to create git worktree")?;

    let relative_workspace = workspace_dir
        .strip_prefix(&git_root)
        .unwrap_or_else(|_| workspace_dir.as_ref());

    let editor_workspace = worktree_path.join(relative_workspace);

    let editor_cfg = global_config.get_editor(&editor_id)?;
    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    save_state(&git_root, &id, &editor_id, relative_workspace)?;

    println!("Worktree created at {}", worktree_path.display());
    println!("Editor opened at {}", editor_workspace.display());

    Ok(())
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
