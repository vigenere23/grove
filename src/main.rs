mod commands;
mod completions;
mod config;
mod editor;
mod git;
mod copy;
mod output;
mod post_create;
mod prompt;
mod setup;

use clap::{builder::ValueHint, CommandFactory, Parser, Subcommand, ValueEnum};
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
    /// Remove a worktree
    #[command(visible_alias = "rm", visible_alias = "remove")]
    Cut {
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
    /// Create a git worktree using workspace config
    #[command(visible_alias = "new", visible_alias = "create")]
    Plant {
        /// Worktree identifier
        id: String,
        /// Base branch to create the worktree from (default: current branch)
        #[arg(long)]
        from: Option<String>,
        /// New branch name to create (default: wip/<id>)
        #[arg(short, long)]
        branch: Option<String>,
        /// Open the editor configured in `.grove.toml` after creating the worktree
        #[arg(long)]
        open: bool,
        /// When using `--open`, launch this editor id from global config instead of `.grove.toml`
        #[arg(short = 'e', long, value_name = "ID", requires = "open")]
        editor: Option<String>,
        /// Override the workspace directory (must contain a .grove.toml)
        #[arg(short, long, value_hint = ValueHint::DirPath)]
        workspace: Option<String>,
    },
    /// Reopen an existing worktree in the editor
    #[command(visible_alias = "open")]
    Tend {
        /// Worktree identifier (interactive selection if omitted)
        id: Option<String>,
        /// Launch this editor id from global config instead of `.grove.toml`
        #[arg(short = 'e', long, value_name = "ID")]
        editor: Option<String>,
        /// Override the workspace directory (must contain a .grove.toml)
        #[arg(short, long, value_hint = ValueHint::DirPath)]
        workspace: Option<String>,
        /// Re-run copy/link and post_create for this workspace even if already set up
        #[arg(long, conflicts_with = "no_setup")]
        setup: bool,
        /// Open without running copy/link or post_create for a new workspace
        #[arg(long)]
        no_setup: bool,
    },
    /// List all worktrees and their status
    #[command(visible_alias = "ls", visible_alias = "list")]
    Trees {
        /// Show full details for each worktree
        #[arg(long)]
        details: bool,
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
        copy: Vec<String>,
        /// Paths to symlink into new worktrees when using this editor (repeatable)
        #[arg(long)]
        link: Vec<String>,
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
                EditorCommands::Add { id, name, command, copy, link } => {
                    commands::config::add_editor(id, name, command, copy, link)
                }
                EditorCommands::Remove { id } => commands::config::remove_editor(id),
            },
        },
        Commands::Cut {
            id,
            force,
            all,
            current,
            branch,
        } => commands::remove::run(id, force, all, current, branch),
        Commands::Plant {
            id,
            from,
            branch,
            workspace,
            open,
            editor,
        } => commands::create::run(id, from, branch, workspace, open, editor),
        Commands::Tend {
            id,
            workspace,
            editor,
            setup,
            no_setup,
        } => commands::open::run(
            id,
            workspace.map(std::path::PathBuf::from),
            editor,
            setup::SetupMode::from_flags(setup, no_setup),
        ),
        Commands::Trees { details } => commands::list::run(details),
    }
}
