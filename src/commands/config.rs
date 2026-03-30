use anyhow::{Context, Result};

use crate::config::global::GlobalConfig;

pub fn list_editors() -> Result<()> {
    let config = GlobalConfig::load().context("failed to load global config")?;

    if config.editors.is_empty() {
        println!("No editors configured.");
        println!("Add one with: parai config editors add <id> --name <name> --command <command>");
        return Ok(());
    }

    for (id, editor) in &config.editors {
        println!("{id}:");
        println!("  name:    {}", editor.name);
        println!("  command: {}", editor.command);
    }

    Ok(())
}

pub fn add_editor(id: String, name: String, command: String) -> Result<()> {
    let mut config = GlobalConfig::load().context("failed to load global config")?;
    config.add_editor(id, name, command)?;
    config.save().context("failed to save global config")?;
    Ok(())
}

pub fn remove_editor(id: String) -> Result<()> {
    let mut config = GlobalConfig::load().context("failed to load global config")?;
    config.remove_editor(&id)?;
    config.save().context("failed to save global config")?;
    Ok(())
}
