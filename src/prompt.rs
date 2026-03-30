use std::io::{self, BufRead, Write};

use anyhow::{bail, Result};

use crate::output::*;

pub fn read_line(prompt: &str) -> Result<String> {
    let mut stdout = io::stdout().lock();
    write!(stdout, "{} ", prompt)?;
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;

    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        bail!("no input provided");
    }
    Ok(trimmed)
}

pub enum BranchConflictChoice {
    Reuse,
    NewName(String),
    Abort,
}

pub fn ask_branch_conflict(branch: &str) -> Result<BranchConflictChoice> {
    println!(
        "{} Branch {} already exists.",
        YELLOW.paint("!"),
        BOLD_CYAN.paint(branch),
    );
    println!("  [r] Reuse the existing branch");
    println!("  [n] Create with a new branch name");
    println!("  [a] Abort");

    let answer = read_line(&format!("{}:", BOLD.paint("Choice [r/n/a]")))?;

    match answer.to_lowercase().as_str() {
        "r" | "reuse" => Ok(BranchConflictChoice::Reuse),
        "n" | "new" => {
            let new_name = read_line(&format!("{}:", BOLD.paint("New branch name")))?;
            Ok(BranchConflictChoice::NewName(new_name))
        }
        "a" | "abort" => Ok(BranchConflictChoice::Abort),
        other => bail!("invalid choice: '{other}' (expected r, n, or a)"),
    }
}

pub enum WorktreeConflictChoice {
    NewName(String),
    OpenExisting,
    Abort,
}

pub fn ask_worktree_conflict(id: &str) -> Result<WorktreeConflictChoice> {
    println!(
        "{} Worktree {} already exists.",
        YELLOW.paint("!"),
        BOLD_CYAN.paint(id),
    );
    println!("  [o] Open the existing worktree");
    println!("  [n] Create with a new name");
    println!("  [a] Abort");

    let answer = read_line(&format!("{}:", BOLD.paint("Choice [o/n/a]")))?;

    match answer.to_lowercase().as_str() {
        "o" | "open" => Ok(WorktreeConflictChoice::OpenExisting),
        "n" | "new" => {
            let new_name = read_line(&format!("{}:", BOLD.paint("New worktree name")))?;
            Ok(WorktreeConflictChoice::NewName(new_name))
        }
        "a" | "abort" => Ok(WorktreeConflictChoice::Abort),
        other => bail!("invalid choice: '{other}' (expected o, n, or a)"),
    }
}
