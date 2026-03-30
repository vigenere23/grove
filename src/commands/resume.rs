use anyhow::{bail, Context, Result};

use crate::config::global::GlobalConfig;
use crate::config::state::RepoState;
use crate::editor;
use crate::git;

pub fn run(id: String) -> Result<()> {
    let git_root = git::repo_root().context("failed to find git repository root")?;
    let state = RepoState::load(&git_root).context("failed to load state")?;

    let wt_state = state.get(&id).context("failed to look up worktree")?;

    let worktree_path = git::worktree_dir(&git_root, &id);
    if !worktree_path.exists() {
        bail!(
            "worktree directory does not exist: {}\nIt may have been removed or pruned.",
            worktree_path.display()
        );
    }

    let global_config = GlobalConfig::load().context("failed to load global config")?;
    let editor_cfg = global_config.get_editor(&wt_state.editor)?;

    let editor_workspace = worktree_path.join(&wt_state.workspace);

    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    println!("Editor opened at {}", editor_workspace.display());

    Ok(())
}
