mod commands;
mod completions;
mod config;
mod editor;
mod git;
mod include;
mod output;
mod prompt;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::env::CompleteEnv;

#[derive(Parser)]
#[command(name = "grove", about = "Manage parallel workspaces with git worktrees")]
pub struct Cli {
    /// Enable verbose output for debugging
    #[arg(short, long, global = true)]
    verbose: bool,
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
        /// Override the workspace directory (must contain a .grove.toml)
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Reopen an existing worktree in the editor
    Open {
        /// Worktree identifier (interactive selection if omitted)
        id: Option<String>,
        /// Override the workspace directory (must contain a .grove.toml)
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
        /// Remove the worktree you are currently inside
        #[arg(long, display_order = 0)]
        current: bool,
        /// Remove all worktrees
        #[arg(long, display_order = 999)]
        all: bool,
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

fn completion_command() -> clap::Command {
    let mut cmd = Cli::command()
        .disable_help_flag(true)
        .mut_arg("verbose", |a| a.hide(true).global(false));

    for subcmd in cmd.get_subcommands_mut() {
        *subcmd = std::mem::take(subcmd).disable_help_flag(true);
    }

    cmd
}

fn main() -> anyhow::Result<()> {
    if std::env::var("COMPLETE").as_deref() == Ok("zsh")
        && !std::env::args_os().any(|a| a == "--")
    {
        print!("zstyle ':completion:*:*:grove:*:*' sort false\n");
    }
    if completions::try_complete_subcommand() {
        std::process::exit(0);
    }
    CompleteEnv::with_factory(completion_command).complete();

    let cli = Cli::parse();
    output::set_verbose(cli.verbose);

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
