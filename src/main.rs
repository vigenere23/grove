mod commands;
mod config;
mod editor;
mod git;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "parai", about = "Run parallel agents on the same workspace, without conflicts")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// List all worktrees and their status
    List,
    /// Reopen an existing worktree in the editor
    Resume {
        /// Worktree identifier
        id: String,
    },
    /// Create a worktree and open an editor
    Start {
        /// Worktree identifier
        id: String,
        /// Base branch to create the worktree from (default: current branch)
        #[arg(long)]
        from: Option<String>,
        /// New branch name to create (default: wip/<id>)
        #[arg(long)]
        branch: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Manage editor configurations
    Editors {
        #[command(subcommand)]
        command: EditorCommands,
    },
}

#[derive(Subcommand)]
enum EditorCommands {
    /// List configured editors
    List,
    /// Add a new editor
    Add {
        /// Unique identifier for the editor
        id: String,
        /// Display name
        #[arg(long)]
        name: String,
        /// Launch command (use $workspace for the workspace path placeholder)
        #[arg(long)]
        command: String,
    },
    /// Remove an editor
    Remove {
        /// Identifier of the editor to remove
        id: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { command } => match command {
            ConfigCommands::Editors { command } => match command {
                EditorCommands::List => commands::config::list_editors(),
                EditorCommands::Add { id, name, command } => {
                    commands::config::add_editor(id, name, command)
                }
                EditorCommands::Remove { id } => commands::config::remove_editor(id),
            },
        },
        Commands::List => commands::list::run(),
        Commands::Resume { id } => commands::resume::run(id),
        Commands::Start { id, from, branch } => commands::start::run(id, from, branch),
    }
}
