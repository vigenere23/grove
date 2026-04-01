use clap::CommandFactory;

use crate::git;
use crate::Cli;

struct WorktreeEntry {
    id: String,
    branch: Option<String>,
}

fn worktree_entries() -> Vec<WorktreeEntry> {
    let Ok(git_root) = git::repo_root() else {
        return vec![];
    };
    let Ok(worktrees) = git::list_worktrees() else {
        return vec![];
    };

    let mut entries: Vec<_> = worktrees
        .iter()
        .filter(|wt| wt.path != git_root)
        .filter_map(|wt| {
            let id = git::worktree_id(&wt.path)?;
            Some(WorktreeEntry {
                id,
                branch: wt.branch.clone(),
            })
        })
        .collect();

    entries.reverse();
    entries
}

fn get_flags(subcmd_name: &str) -> Vec<(String, String)> {
    let cmd = Cli::command();
    let subcmd = cmd
        .get_subcommands()
        .find(|s| s.get_name() == subcmd_name || s.get_all_aliases().any(|a| a == subcmd_name))
        .expect("subcommand must exist");

    subcmd
        .get_arguments()
        .filter(|a| a.get_long().is_some())
        .map(|a| {
            let flag = format!("--{}", a.get_long().unwrap());
            let help = a
                .get_help()
                .map(|h| h.to_string())
                .unwrap_or_default();
            (flag, help)
        })
        .collect()
}

/// Returns true if this was a completion request we handled.
pub fn try_complete_subcommand() -> bool {
    let Ok(shell) = std::env::var("COMPLETE") else {
        return false;
    };
    if shell.is_empty() || shell == "0" {
        return false;
    }
    let args: Vec<String> = std::env::args().collect();
    let Some(sep) = args.iter().position(|a| a == "--") else {
        return false;
    };

    let cmd_args = &args[sep + 1..];
    if cmd_args.len() < 3 {
        return false;
    }
    let subcmd = &cmd_args[1];
    let current = &cmd_args[cmd_args.len() - 1];
    let prev = &cmd_args[cmd_args.len() - 2];

    if prev == "--workspace" || prev == "-w" {
        return false;
    }

    match subcmd.as_str() {
        "rm" | "remove" => {
            complete_rm(current);
            true
        }
        "open" => {
            complete_open(current);
            true
        }
        _ => false,
    }
}

/// Completion order: --current, worktrees (newest first), --all.
/// When typing a `-` prefix, show all flags.
fn complete_rm(current: &str) {
    let flags = get_flags("rm");

    let mut lines: Vec<(String, String)> = Vec::new();

    if current.starts_with('-') {
        for (flag, desc) in &flags {
            if flag.starts_with(current) {
                lines.push((flag.clone(), desc.clone()));
            }
        }
    } else {
        let promoted = ["--current"];
        let demoted = ["--all"];

        for name in promoted {
            if let Some((f, d)) = flags.iter().find(|(f, _)| f == name) {
                lines.push((f.clone(), d.clone()));
            }
        }

        for entry in &worktree_entries() {
            if entry.id.starts_with(current) {
                lines.push((
                    entry.id.clone(),
                    entry.branch.clone().unwrap_or_default(),
                ));
            }
        }

        for name in demoted {
            if let Some((f, d)) = flags.iter().find(|(f, _)| f == name) {
                lines.push((f.clone(), d.clone()));
            }
        }
    }

    print_completions(&lines);
}

fn complete_open(current: &str) {
    let lines: Vec<(String, String)> = worktree_entries()
        .iter()
        .filter(|e| e.id.starts_with(current))
        .map(|e| {
            (
                e.id.clone(),
                e.branch.clone().unwrap_or_default(),
            )
        })
        .collect();

    print_completions(&lines);
}

fn print_completions(lines: &[(String, String)]) {
    for (value, desc) in lines {
        if desc.is_empty() {
            println!("{value}");
        } else {
            println!("{value}:{desc}");
        }
    }
}
