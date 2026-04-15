use std::env;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::config::global::GlobalConfig;
use crate::output::*;

pub fn edit() -> Result<()> {
    let path = GlobalConfig::path().context("failed to determine config path")?;

    if !path.exists() {
        let config = GlobalConfig::default();
        config.save().context("failed to create default config")?;
    }

    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let parts =
        shell_words::split(&editor).context("failed to parse $EDITOR/$VISUAL command")?;
    let (program, args) = parts
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("$EDITOR/$VISUAL is empty"))?;

    let status = Command::new(program)
        .args(args)
        .arg(&path)
        .status()
        .with_context(|| format!("failed to launch editor '{editor}'"))?;

    if !status.success() {
        bail!("editor exited with {status}");
    }

    println!(
        "{} Config saved at {}",
        GREEN.paint("✔"),
        path.display(),
    );

    Ok(())
}

pub fn list_editors() -> Result<()> {
    let config = GlobalConfig::load().context("failed to load global config")?;

    if !config.copy.is_empty() {
        println!(
            "{} {}",
            DIM.paint("copy:"),
            config.copy.join(", "),
        );
    }
    if !config.link.is_empty() {
        println!(
            "{} {}",
            DIM.paint("link:"),
            config.link.join(", "),
        );
    }
    if !config.copy.is_empty() || !config.link.is_empty() {
        println!();
    }

    if config.editors.is_empty() {
        println!("{}", DIM.paint("No editors configured."));
        println!(
            "Add one with: {}",
            BOLD.paint("grove config editors add <id> --name <name> --command <command>")
        );
        return Ok(());
    }

    for (id, editor) in &config.editors {
        println!("{}:", BOLD_CYAN.paint(id));
        println!("  {}    {}", DIM.paint("name:"), editor.name);
        println!("  {} {}", DIM.paint("command:"), editor.command);
        if !editor.copy.is_empty() {
            println!("  {} {}", DIM.paint("copy:"), editor.copy.join(", "));
        }
        if !editor.link.is_empty() {
            println!("  {} {}", DIM.paint("link:"), editor.link.join(", "));
        }
    }

    Ok(())
}

pub fn add_editor(
    id: String,
    name: String,
    command: String,
    copy: Vec<String>,
    link: Vec<String>,
) -> Result<()> {
    let mut config = GlobalConfig::load().context("failed to load global config")?;
    config.add_editor(id.clone(), name, command, copy, link)?;
    config.save().context("failed to save global config")?;
    println!("{} Editor {} added", GREEN.paint("✔"), BOLD_CYAN.paint(&id));
    Ok(())
}

pub fn remove_editor(id: String) -> Result<()> {
    let mut config = GlobalConfig::load().context("failed to load global config")?;
    config.remove_editor(&id)?;
    config.save().context("failed to save global config")?;
    println!("{} Editor {} removed", GREEN.paint("✔"), BOLD.paint(&id));
    Ok(())
}
