use std::io;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::output::*;

#[cfg(unix)]
fn spawn_shell(working_dir: &Path, cmd: &str) -> io::Result<std::process::ExitStatus> {
    Command::new("sh")
        .args(["-c", cmd])
        .current_dir(working_dir)
        .status()
}

#[cfg(windows)]
fn spawn_shell(working_dir: &Path, cmd: &str) -> io::Result<std::process::ExitStatus> {
    Command::new("cmd")
        .args(["/C", cmd])
        .current_dir(working_dir)
        .status()
}

#[cfg(not(any(unix, windows)))]
fn spawn_shell(working_dir: &Path, cmd: &str) -> io::Result<std::process::ExitStatus> {
    let _ = (working_dir, cmd);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "post-create commands are only supported on Unix and Windows",
    ))
}

/// Run each shell command in order from `working_dir`, inheriting stdin/stdout/stderr.
/// Prints a line before each command. Fails on the first non-zero exit or spawn error.
pub fn run(working_dir: &Path, commands: &[String]) -> Result<()> {
    for cmd in commands {
        println!("  {} {}", DIM.paint("run"), cmd);

        let status = spawn_shell(working_dir, cmd)
            .with_context(|| format!("failed to spawn post-create command: {cmd}"))?;

        if !status.success() {
            bail!("post-create command failed ({status}): {cmd}");
        }
    }

    Ok(())
}
