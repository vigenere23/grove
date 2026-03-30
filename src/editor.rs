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
    let workspace_str = workspace_path.to_string_lossy();
    let expanded = editor.command.replace("$workspace", &workspace_str);

    let parts = shell_words::split(&expanded).map_err(|e| EditorError::Parse(e.to_string()))?;
    let (program, args) = parts
        .split_first()
        .ok_or(EditorError::EmptyCommand)?;

    Command::new(program)
        .args(args)
        .spawn()
        .map_err(|e| EditorError::Launch {
            command: expanded.clone(),
            source: e,
        })?;

    Ok(())
}
