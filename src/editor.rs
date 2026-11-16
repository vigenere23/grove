use std::path::Path;
use std::process::Command;

use thiserror::Error;

use crate::config::global::EditorConfig;

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("editor command is empty")]
    EmptyCommand,

    #[error("failed to parse editor command: {0}")]
    Parse(String),

    #[error("failed to launch editor '{command}': {source}")]
    Launch {
        command: String,
        source: std::io::Error,
    },
}

/// Replace `$workspace` in the editor command template and spawn the process.
pub fn open(editor: &EditorConfig, workspace_path: &Path) -> Result<(), EditorError> {
    use crate::output::verbose;

    let workspace_str = workspace_path.to_string_lossy();
    let expanded = editor.command.replace("$workspace", &workspace_str);
    verbose!("editor command expanded: {}", expanded);

    let parts = shell_words::split(&expanded).map_err(|e| EditorError::Parse(e.to_string()))?;
    let (program, args) = parts
        .split_first()
        .ok_or(EditorError::EmptyCommand)?;

    verbose!("spawning: {} {:?}", program, args);

    Command::new(program)
        .args(args)
        .spawn()
        .map_err(|e| EditorError::Launch {
            command: expanded.clone(),
            source: e,
        })?;

    Ok(())
}

/// Hand the current terminal over to the user's shell. Only returns on failure.
#[cfg(unix)]
pub fn exec_shell(workspace_path: &Path) -> EditorError {
    use std::os::unix::process::CommandExt;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let source = Command::new(&shell).current_dir(workspace_path).exec();
    EditorError::Launch {
        command: shell,
        source,
    }
}

/// No `exec` outside Unix: wait for the shell, then exit with its status.
#[cfg(not(unix))]
pub fn exec_shell(workspace_path: &Path) -> EditorError {
    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    match Command::new(&shell).current_dir(workspace_path).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(source) => EditorError::Launch {
            command: shell,
            source,
        },
    }
}
