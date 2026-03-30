use anyhow::{Context, Result};

use crate::config::global::GlobalConfig;
use crate::output::*;

pub fn list_editors() -> Result<()> {
    let config = GlobalConfig::load().context("failed to load global config")?;

    if config.editors.is_empty() {
        println!("{}", DIM.paint("No editors configured."));
        println!(
            "Add one with: {}",
            BOLD.paint("parai config editors add <id> --name <name> --command <command>")
        );
        return Ok(());
    }

    for (id, editor) in &config.editors {
        println!("{}:", BOLD_CYAN.paint(id));
        println!("  {}    {}", DIM.paint("name:"), editor.name);
        println!("  {} {}", DIM.paint("command:"), editor.command);
    }

    Ok(())
}

pub fn add_editor(id: String, name: String, command: String) -> Result<()> {
    let mut config = GlobalConfig::load().context("failed to load global config")?;
    config.add_editor(id.clone(), name, command)?;
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
