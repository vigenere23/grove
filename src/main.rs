mod commands;
mod config;
mod editor;
mod git;
mod include;
mod output;
mod prompt;

use clap::{Parser, Subcommand, ValueEnum};

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
    #[command(alias = "list")]
    Ls {
        /// Show full details for each worktree
        #[arg(long)]
        details: bool,
    },
    /// Create a worktree and open an editor
    New {
        /// Worktree identifier
        id: String,
        /// Base branch to create the worktree from (default: current branch)
        #[arg(long)]
        from: Option<String>,
        /// New branch name to create (default: wip/<id>)
        #[arg(short, long)]
        branch: Option<String>,
        /// Override the workspace directory (must contain a .parai.toml)
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Reopen an existing worktree in the editor
    Open {
        /// Worktree identifier (interactive selection if omitted)
        id: Option<String>,
        /// Override the workspace directory (must contain a .parai.toml)
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Remove a worktree
    #[command(alias = "remove")]
    Rm {
        /// Worktree identifier (omit when using --all or --current)
        id: Option<String>,
        /// Remove even if the worktree has uncommitted changes
        #[arg(long)]
        force: bool,
        /// Remove all worktrees
        #[arg(long)]
        all: bool,
        /// Remove the worktree you are currently inside
        #[arg(long)]
        current: bool,
        /// What to do with the branch after removal
        #[arg(long, value_enum)]
        branch: Option<BranchCleanup>,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Open the global config file in your editor
    Edit,
    /// Manage editor configurations
    Editors {
        #[command(subcommand)]
        command: EditorCommands,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum BranchCleanup {
    /// Delete the local branch
    Delete,
    /// Delete the local and remote branch
    DeleteRemote,
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
        /// Paths to copy into new worktrees when using this editor (repeatable)
        #[arg(long)]
        include: Vec<String>,
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
            ConfigCommands::Edit => commands::config::edit(),
            ConfigCommands::Editors { command } => match command {
                EditorCommands::List => commands::config::list_editors(),
                EditorCommands::Add { id, name, command, include } => {
                    commands::config::add_editor(id, name, command, include)
                }
                EditorCommands::Remove { id } => commands::config::remove_editor(id),
            },
        },
        Commands::Ls { details } => commands::ls::run(details),
        Commands::New { id, from, branch, workspace } => {
            commands::new::run(id, from, branch, workspace)
        }
        Commands::Open { id, workspace } => commands::open::run(id, workspace),
        Commands::Rm {
            id,
            force,
            all,
            current,
            branch,
        } => commands::rm::run(id, force, all, current, branch),
    }
}
