use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::commands;
use crate::config::global::GlobalConfig;
use crate::config::state::RepoState;
use crate::config::workspace::{self, WorkspaceConfigError};
use crate::editor;
use crate::git;
use crate::output::*;
use crate::prompt::{self, SelectItem};
use crate::setup::{self, SetupMode};

pub fn run(
    id: Option<String>,
    workspace_override: Option<PathBuf>,
    editor_override: Option<String>,
    terminal: bool,
    mode: SetupMode,
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
        .find(|wt| git::worktree_matches_id(&git_root, &wt.path, &id))
        .with_context(|| format!("no worktree found with id '{id}'"))?;

    let global_config = GlobalConfig::load().context("failed to load global config")?;

    // ── Which workspace? ────────────────────────────────────────────────
    let relative = match workspace_override {
        Some(ref ws) => resolve_override(&git_root, &worktree_paths, ws)?,
        None => select_workspace(&git_root, &state, &id)?,
    };
    verbose!("relative workspace: {}", relative.display());

    // ── Provision it? ───────────────────────────────────────────────────
    // Owned `bool`: `state.get` borrows immutably, `insert_workspace` needs `&mut`.
    let is_new = state.get(&id).map(|wt| !wt.contains(&relative)).unwrap_or(true);
    let mut do_provision = match mode {
        SetupMode::Never => false,
        SetupMode::Force => true,
        SetupMode::Auto => is_new,
    };
    // Membership means "provisioned", so `--no-setup` on a new workspace deliberately
    // records nothing. The missing-config case below is the one exception.
    let mut record_membership = do_provision;

    // A recorded workspace's `.grove.toml` may have been deleted since; check the file
    // rather than downcasting the loader error further down.
    let config_path = git_root.join(&relative).join(workspace::CONFIG_FILENAME);
    if do_provision && !config_path.is_file() {
        if matches!(mode, SetupMode::Force) {
            bail!(
                "cannot set up workspace '{}': no .grove.toml at {}",
                workspace::display_workspace(&relative),
                config_path.display(),
            );
        }
        eprintln!(
            "{} No .grove.toml at {} — opening without setup.",
            YELLOW.paint("!"),
            config_path.display(),
        );
        do_provision = false;
        // Record it anyway, so the warning is not reprinted on every future open.
        record_membership = true;
    }

    // Loaded lazily on purpose: `grove tend <id> -e <editor>` must keep working for an
    // already-provisioned workspace whose `.grove.toml` is gone.
    let plan = if do_provision {
        Some(setup::plan_for_relative(&git_root, &global_config, &relative)?)
    } else {
        None
    };

    // ── Which editor? ───────────────────────────────────────────────────
    let editor_id = match editor_override {
        _ if terminal => None,
        Some(ref o) => {
            global_config.get_editor(o)?;
            Some(o.clone())
        }
        None => {
            let from_config = match plan {
                // Already read the config for the plan; do not read it twice.
                Some(ref plan) => plan.editor.clone(),
                None => workspace::load_workspace_config(&git_root.join(&relative))
                    .context("failed to load workspace config from source repo")?
                    .config
                    .editor,
            };
            match from_config {
                Some(editor_id) => {
                    global_config.get_editor(&editor_id)?;
                    Some(editor_id)
                }
                None => bail!(
                    "no `editor` in .grove.toml{} — set `editor` to an id from `grove config editors list`, or pass --editor <id>",
                    match workspace_override {
                        Some(ref ws) => format!(" under '{}'", ws.display()),
                        None => String::new(),
                    },
                ),
            }
        }
    };

    verbose!(
        "editor_id: {:?}, relative_workspace: {}",
        editor_id,
        relative.display()
    );

    let editor_cfg = editor_id
        .as_deref()
        .map(|id| global_config.get_editor(id))
        .transpose()?;
    let editor_workspace = worktree_info.path.join(&relative);
    verbose!("editor_workspace: {}", editor_workspace.display());

    // ── Open, provisioning first if this pair is new ─────────────────────
    // State is saved before the fragile steps: copy/link and post_create can fail
    // half-way, and losing the record would re-run them on the next open.
    if record_membership {
        state.insert_workspace(&id, &relative);
        state.save(&git_root).context("failed to save state")?;
    }

    if let Some(ref plan) = plan {
        println!(
            "{} Setting up workspace {} …",
            CYAN.paint("→"),
            BOLD_CYAN.paint(workspace::display_workspace(&relative)),
        );
        setup::provision_files(plan, &worktree_info.path);
    }

    let Some(editor_cfg) = editor_cfg else {
        if let Some(ref plan) = plan {
            setup::run_post_create(plan, &worktree_info.path)?;
        }
        return commands::create::open_shell(&editor_workspace);
    };

    editor::open(editor_cfg, &editor_workspace).context("failed to open editor")?;

    println!(
        "{} Editor opened for {} at {}",
        GREEN.paint("✔"),
        BOLD_CYAN.paint(&id),
        editor_workspace.display()
    );

    if let Some(ref plan) = plan {
        setup::run_post_create(plan, &worktree_info.path)?;
    }

    Ok(())
}

/// `-w <dir>`: the directory is relative to the cwd, which may sit inside another
/// worktree — hence `worktree_paths`, so the result is still relative and not absolute.
fn resolve_override(
    git_root: &Path,
    worktree_paths: &[PathBuf],
    ws: &Path,
) -> Result<PathBuf> {
    let cwd = env::current_dir().context("failed to determine current working directory")?;
    let ws_path = cwd.join(ws);
    verbose!("workspace override: {}", ws_path.display());

    // Loading the config canonicalizes the directory and rejects a bad `-w`. A *missing*
    // config over an existing directory is not fatal here: the workspace may have been
    // provisioned before its `.grove.toml` was deleted, and the caller decides what to do
    // about that. An invalid config, or a directory that does not exist, still fails.
    let config_dir = match workspace::load_workspace_config(&ws_path) {
        Ok(resolved) => resolved.config_dir,
        Err(WorkspaceConfigError::NotFound(dir)) if dir.is_dir() => dir,
        Err(e) => {
            return Err(e).with_context(|| {
                format!("failed to load workspace config from '{}'", ws.display())
            });
        }
    };

    git::relative_workspace(git_root, worktree_paths, &config_dir)
        .context("failed to resolve the workspace relative to the git root")
}

/// Every workspace in the main checkout, plus any this worktree has been provisioned at
/// whose `.grove.toml` has since disappeared. Provisioned ones come first, alphabetical
/// within each group. A single candidate is used without prompting.
fn select_workspace(git_root: &Path, state: &RepoState, id: &str) -> Result<PathBuf> {
    let recorded: &[PathBuf] = state
        .get(id)
        .map(|wt| wt.workspaces())
        .unwrap_or_default();

    let mut found = workspace::scan_workspaces(git_root)
        .with_context(|| format!("failed to scan {} for .grove.toml files", git_root.display()))?;
    verbose!("scanned workspaces: {:?}", found);
    for ws in recorded {
        if !found.contains(ws) {
            found.push(ws.clone());
        }
    }

    let mut candidates: Vec<(bool, PathBuf)> = found
        .into_iter()
        .map(|ws| (recorded.contains(&ws), ws))
        .collect();
    candidates.sort_by(|(a_set, a), (b_set, b)| b_set.cmp(a_set).then_with(|| a.cmp(b)));

    match candidates.len() {
        0 => bail!(
            "no .grove.toml found under {} — add one (optional `editor` = id from `grove config editors list`), or pass -w/--workspace",
            git_root.display(),
        ),
        1 => return Ok(candidates.remove(0).1),
        _ => {}
    }

    let items: Vec<SelectItem> = candidates
        .iter()
        .enumerate()
        .map(|(i, (provisioned, ws))| SelectItem {
            // The root workspace's relative path is empty, so an index is the only
            // unambiguous id here.
            id: i.to_string(),
            label: format!(
                "{}  ({})",
                workspace::display_workspace(ws),
                if *provisioned { "set up" } else { "new — will run setup" },
            ),
            ..Default::default()
        })
        .collect();

    let chosen = prompt::select("Open workspace:", &items)?;
    let chosen: usize = chosen
        .parse()
        .with_context(|| format!("unexpected selection '{chosen}'"))?;

    Ok(candidates.swap_remove(chosen).1)
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
                .find(|(id, _)| git::worktree_matches_id(git_root, &wt.path, id))
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
