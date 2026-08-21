use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::global::GlobalConfig;
use crate::config::workspace::{self, ResolvedWorkspace};
use crate::copy;
use crate::git;
use crate::output::*;
use crate::post_create;

/// Everything needed to set a worktree up for one workspace: where to copy and link
/// from, which paths, and what to run afterwards.
pub struct WorkspacePlan {
    /// Editor id from the workspace `.grove.toml`, if it sets one.
    pub editor: Option<String>,
    /// Absolute directory in the main checkout holding the `.grove.toml`. It is the
    /// source of the copies and the target of the symlinks.
    pub source_dir: PathBuf,
    /// The same workspace expressed relative to the git root — the value stored in state.
    pub relative: PathBuf,
    pub copy: Vec<String>,
    pub link: Vec<String>,
    pub post_create: Vec<String>,
}

/// Whether provisioning should run for a workspace that is being opened.
pub enum SetupMode {
    /// Provision the first time a (worktree, workspace) pair is opened.
    Auto,
    /// Re-run provisioning even for an already provisioned workspace.
    Force,
    /// Open without provisioning.
    Never,
}

impl SetupMode {
    pub fn from_flags(setup: bool, no_setup: bool) -> Self {
        match (setup, no_setup) {
            (true, _) => Self::Force,
            (_, true) => Self::Never,
            _ => Self::Auto,
        }
    }
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

/// A path that does not exist yet degrades to itself rather than failing.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Merge global + editor-specific + workspace `copy`/`link` and finish the plan.
fn finish_plan(
    global: &GlobalConfig,
    resolved: ResolvedWorkspace,
    relative: PathBuf,
) -> Result<WorkspacePlan> {
    // NOTE: the merge key is the workspace `.grove.toml` `editor` only — a `-e/--editor`
    // override never contributes that editor's `copy`/`link` entries. That is a
    // pre-existing inconsistency with the docs, kept verbatim here on purpose.
    let copy_key = resolved.config.editor.as_deref().unwrap_or("");

    let mut copy_paths = global.resolved_copy(copy_key);
    verbose!("resolved copy (global+editor): {:?}", copy_paths);
    merge_workspace_paths(&mut copy_paths, &resolved.config.copy);
    verbose!("final copy paths: {:?}", copy_paths);

    let mut link_paths = global.resolved_link(copy_key);
    verbose!("resolved link (global+editor): {:?}", link_paths);
    merge_workspace_paths(&mut link_paths, &resolved.config.link);
    verbose!("final link paths: {:?}", link_paths);

    ensure_copy_and_link_disjoint(&copy_paths, &link_paths)?;

    Ok(WorkspacePlan {
        editor: resolved.config.editor,
        source_dir: resolved.config_dir,
        relative,
        copy: copy_paths,
        link: link_paths,
        post_create: resolved.config.post_create,
    })
}

/// Resolve the workspace to act on: `-w <dir>` if given, otherwise the nearest
/// `.grove.toml` walking `cwd` up to `git_root`.
pub fn resolve_plan(
    git_root: &Path,
    global: &GlobalConfig,
    cwd: &Path,
    workspace_override: Option<&Path>,
) -> Result<WorkspacePlan> {
    let resolved = if let Some(ws) = workspace_override {
        let ws_path = cwd.join(ws);
        verbose!("workspace override: {}", ws_path.display());
        let resolved = workspace::load_workspace_config(&ws_path).with_context(|| {
            format!("failed to load workspace config from '{}'", ws.display())
        })?;
        if let Some(ref editor_id) = resolved.config.editor {
            global.get_editor(editor_id)?;
        }
        verbose!(
            "workspace config editor={:?}, copy={:?}, link={:?}, post_create={:?}, config_dir={}",
            resolved.config.editor,
            resolved.config.copy,
            resolved.config.link,
            resolved.config.post_create,
            resolved.config_dir.display()
        );
        resolved
    } else {
        match workspace::find_workspace_config(cwd, git_root)
            .context("failed to read workspace config")?
        {
            Some(resolved) => {
                if let Some(ref editor_id) = resolved.config.editor {
                    global.get_editor(editor_id)?;
                }
                verbose!(
                    "workspace config found: editor={:?}, copy={:?}, link={:?}, post_create={:?}, config_dir={}",
                    resolved.config.editor,
                    resolved.config.copy,
                    resolved.config.link,
                    resolved.config.post_create,
                    resolved.config_dir.display()
                );
                resolved
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

    // The workspace dir comes from the cwd or from `-w`, both of which should resolve
    // inside the main checkout, so the other worktrees need not be considered here.
    // If it somehow does not, erroring beats silently storing an absolute path.
    let relative = git::relative_workspace(git_root, &[], &resolved.config_dir)
        .context("failed to resolve the workspace relative to the git root")?;
    verbose!("relative_workspace: {}", relative.display());

    finish_plan(global, resolved, relative)
}

/// Plan for an already-known git-root-relative workspace, reading its `.grove.toml`
/// from the main checkout at `git_root.join(relative)`.
pub fn plan_for_relative(
    git_root: &Path,
    global: &GlobalConfig,
    relative: &Path,
) -> Result<WorkspacePlan> {
    let source_dir = git_root.join(relative);
    verbose!("workspace source: {}", source_dir.display());

    let resolved = workspace::load_workspace_config(&source_dir).with_context(|| {
        format!(
            "failed to load workspace config from '{}'",
            source_dir.display()
        )
    })?;
    if let Some(ref editor_id) = resolved.config.editor {
        global.get_editor(editor_id)?;
    }
    verbose!(
        "workspace config editor={:?}, copy={:?}, link={:?}, post_create={:?}, config_dir={}",
        resolved.config.editor,
        resolved.config.copy,
        resolved.config.link,
        resolved.config.post_create,
        resolved.config_dir.display()
    );

    finish_plan(global, resolved, relative.to_path_buf())
}

/// Symlink, then copy, the planned paths into `<worktree>/<relative>`. Non-fatal:
/// individual failures warn and the rest continue, matching [`crate::copy`].
pub fn provision_files(plan: &WorkspacePlan, worktree_path: &Path) {
    let dest = worktree_path.join(&plan.relative);

    // Belt and braces: `copy::copy_paths` removes each destination before copying, so a
    // source that is also the destination would destroy the very files it reads.
    if canonical(&plan.source_dir) == canonical(&dest) {
        verbose!(
            "skipping provisioning: source and destination are the same directory ({})",
            dest.display()
        );
        return;
    }

    copy::link_paths(&plan.source_dir, &dest, &plan.link);
    copy::copy_paths(&plan.source_dir, &dest, &plan.copy);
}

/// Run the workspace's `post_create` commands from `<worktree>/<relative>`.
pub fn run_post_create(plan: &WorkspacePlan, worktree_path: &Path) -> Result<()> {
    if plan.post_create.is_empty() {
        return Ok(());
    }

    let dest = worktree_path.join(&plan.relative);
    post_create::run(&dest, &plan.post_create).context("post-create command")
}
