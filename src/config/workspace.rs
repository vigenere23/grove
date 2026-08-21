use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::output::verbose;

#[derive(Debug, Error)]
pub enum WorkspaceConfigError {
    #[error("failed to read workspace config at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid workspace config at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("no .grove.toml found in {0}")]
    NotFound(PathBuf),
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub editor: Option<String>,
    #[serde(default)]
    pub copy: Vec<String>,
    #[serde(default)]
    pub link: Vec<String>,
    /// Shell commands to run after copy/link and after `--open` (if used). Workspace config only.
    #[serde(default)]
    pub post_create: Vec<String>,
}

pub struct ResolvedWorkspace {
    pub config: WorkspaceConfig,
    /// The directory containing the `.grove.toml` that was selected.
    pub config_dir: PathBuf,
}

pub const CONFIG_FILENAME: &str = ".grove.toml";

/// Load the workspace config from a specific directory.
/// Returns an error if `.grove.toml` does not exist there.
pub fn load_workspace_config(dir: &Path) -> Result<ResolvedWorkspace, WorkspaceConfigError> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let config_path = dir.join(CONFIG_FILENAME);

    if !config_path.is_file() {
        return Err(WorkspaceConfigError::NotFound(dir));
    }

    let contents =
        fs::read_to_string(&config_path).map_err(|e| WorkspaceConfigError::Read {
            path: config_path.clone(),
            source: e,
        })?;

    let config: WorkspaceConfig =
        toml::from_str(&contents).map_err(|e| WorkspaceConfigError::Parse {
            path: config_path,
            source: e,
        })?;

    Ok(ResolvedWorkspace {
        config,
        config_dir: dir,
    })
}

/// Walk from `start` up to (and including) `git_root`, looking for `.grove.toml`.
/// Returns the most specific (closest to `start`) config found.
pub fn find_workspace_config(
    start: &Path,
    git_root: &Path,
) -> Result<Option<ResolvedWorkspace>, WorkspaceConfigError> {
    let start = start
        .canonicalize()
        .unwrap_or_else(|_| start.to_path_buf());
    let git_root = git_root
        .canonicalize()
        .unwrap_or_else(|_| git_root.to_path_buf());

    let mut current = Some(start.as_path());

    while let Some(dir) = current {
        let config_path = dir.join(CONFIG_FILENAME);

        if config_path.is_file() {
            let contents =
                fs::read_to_string(&config_path).map_err(|e| WorkspaceConfigError::Read {
                    path: config_path.clone(),
                    source: e,
                })?;

            let config: WorkspaceConfig =
                toml::from_str(&contents).map_err(|e| WorkspaceConfigError::Parse {
                    path: config_path,
                    source: e,
                })?;

            return Ok(Some(ResolvedWorkspace {
                config,
                config_dir: dir.to_path_buf(),
            }));
        }

        if dir == git_root {
            break;
        }

        current = dir.parent();
    }

    Ok(None)
}

/// Directories never descended into while scanning: build output and vendored
/// dependency trees, which are large and never hold a workspace config.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "vendor",
    ".venv",
];

/// Walk down from `root`, collecting every directory that contains a `.grove.toml`.
/// Returns the paths relative to `root`, sorted; `root` itself is `PathBuf::from("")`.
///
/// Prunes `.git`, [`SKIP_DIRS`], and grove's own worktree stores (`.grove__*`, normally a
/// sibling of the git root but pruned defensively). A [`SKIP_DIRS`] directory is still
/// recorded when it holds a `.grove.toml` of its own — a package really named `build` or
/// `dist` is a workspace — it is simply never descended into. Symlinked directories are
/// not followed, so the walk cannot cycle. A subdirectory that cannot be read is skipped
/// rather than failing the scan; only an unreadable `root` is an error.
pub fn scan_workspaces(root: &Path) -> Result<Vec<PathBuf>, WorkspaceConfigError> {
    let mut found = Vec::new();
    let mut queue = vec![(root.to_path_buf(), PathBuf::new())];

    while let Some((dir, relative)) = queue.pop() {
        if dir.join(CONFIG_FILENAME).is_file() {
            found.push(relative.clone());
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if relative.as_os_str().is_empty() => {
                return Err(WorkspaceConfigError::Read { path: dir, source });
            }
            // One permission-denied directory must not abort a whole-monorepo scan.
            Err(source) => {
                verbose!("skipping unreadable directory {}: {source}", dir.display());
                continue;
            }
        };

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name == ".git" || name.starts_with(".grove__") {
                continue;
            }

            // `file_type` does not traverse symlinks, so symlinked directories are skipped.
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }

            // A heavy directory is never walked into, but one holding its own
            // `.grove.toml` is a genuine workspace and must not vanish from the list.
            if SKIP_DIRS.contains(&name.as_ref()) {
                if entry.path().join(CONFIG_FILENAME).is_file() {
                    found.push(relative.join(name.as_ref()));
                }
                continue;
            }

            queue.push((entry.path(), relative.join(name.as_ref())));
        }
    }

    found.sort();
    Ok(found)
}

/// Render a git-root-relative workspace path for display. The git root itself
/// relativizes to an empty path, which shows as `.`.
pub fn display_workspace(relative: &Path) -> String {
    let rendered = relative.display().to_string();
    if rendered.is_empty() {
        ".".to_string()
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Fresh, deterministically named scratch directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("grove-scan-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn workspace(root: &Path, relative: &str) {
        let dir = root.join(relative);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CONFIG_FILENAME), "copy = []\n").unwrap();
    }

    fn rels(found: &[PathBuf]) -> Vec<String> {
        found.iter().map(|p| p.display().to_string()).collect()
    }

    #[test]
    fn scans_root_only() {
        let root = scratch("scans_root_only");
        fs::write(root.join(CONFIG_FILENAME), "copy = []\n").unwrap();

        assert_eq!(rels(&scan_workspaces(&root).unwrap()), vec![""]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scans_nested_workspaces() {
        let root = scratch("scans_nested_workspaces");
        fs::write(root.join(CONFIG_FILENAME), "copy = []\n").unwrap();
        workspace(&root, "apps/api");
        workspace(&root, "packages/ui");

        assert_eq!(
            rels(&scan_workspaces(&root).unwrap()),
            vec!["", "apps/api", "packages/ui"]
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn prunes_skip_dirs() {
        let root = scratch("prunes_skip_dirs");
        workspace(&root, "apps/api");
        for skipped in SKIP_DIRS {
            workspace(&root, &format!("{skipped}/pkg"));
        }
        workspace(&root, ".git/hooks");
        workspace(&root, ".grove__repo/wt");

        assert_eq!(rels(&scan_workspaces(&root).unwrap()), vec!["apps/api"]);

        fs::remove_dir_all(&root).unwrap();
    }

    /// A pruned directory that holds its own config is a real workspace (a package named
    /// `build`, say): it is listed, but still not descended into.
    #[test]
    fn records_pruned_dir_holding_a_config() {
        let root = scratch("records_pruned_dir_holding_a_config");
        workspace(&root, "apps/api");
        workspace(&root, "build");
        workspace(&root, "build/nested");
        workspace(&root, "node_modules/pkg");

        assert_eq!(
            rels(&scan_workspaces(&root).unwrap()),
            vec!["apps/api", "build"]
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn omits_directories_without_config() {
        let root = scratch("omits_directories_without_config");
        workspace(&root, "apps/api");
        fs::create_dir_all(root.join("apps/web/src")).unwrap();

        let found = scan_workspaces(&root).unwrap();
        assert_eq!(rels(&found), vec!["apps/api"]);
        assert!(!found.iter().any(|p| p == Path::new("apps")));

        fs::remove_dir_all(&root).unwrap();
    }
}
