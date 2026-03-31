use std::io::{self, BufRead, Write};

use anyhow::{bail, Result};
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

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

    let items = vec![
        SelectItem { id: "r".into(), label: "Reuse the existing branch".into(), key: Some('r'), ..Default::default() },
        SelectItem { id: "n".into(), label: "Create with a new branch name".into(), key: Some('n'), ..Default::default() },
        SelectItem { id: "a".into(), label: "Abort".into(), key: Some('a'), ..Default::default() },
    ];

    match select("Choose:", &items)?.as_str() {
        "r" => Ok(BranchConflictChoice::Reuse),
        "n" => {
            let new_name = read_line(&format!("{}:", BOLD.paint("New branch name")))?;
            Ok(BranchConflictChoice::NewName(new_name))
        }
        _ => Ok(BranchConflictChoice::Abort),
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

    let items = vec![
        SelectItem { id: "o".into(), label: "Open the existing worktree".into(), key: Some('o'), ..Default::default() },
        SelectItem { id: "n".into(), label: "Create with a new name".into(), key: Some('n'), ..Default::default() },
        SelectItem { id: "a".into(), label: "Abort".into(), key: Some('a'), ..Default::default() },
    ];

    match select("Choose:", &items)?.as_str() {
        "o" => Ok(WorktreeConflictChoice::OpenExisting),
        "n" => {
            let new_name = read_line(&format!("{}:", BOLD.paint("New worktree name")))?;
            Ok(WorktreeConflictChoice::NewName(new_name))
        }
        _ => Ok(WorktreeConflictChoice::Abort),
    }
}

pub enum BothConflictChoice {
    OpenExisting,
    NewName(String),
    Abort,
}

pub fn ask_both_conflict(id: &str, branch: &str) -> Result<BothConflictChoice> {
    println!(
        "{} Worktree {} and branch {} both already exist.",
        YELLOW.paint("!"),
        BOLD_CYAN.paint(id),
        BOLD_CYAN.paint(branch),
    );

    let items = vec![
        SelectItem { id: "o".into(), label: "Open the existing worktree".into(), key: Some('o'), ..Default::default() },
        SelectItem { id: "n".into(), label: "Create with a new worktree name".into(), key: Some('n'), ..Default::default() },
        SelectItem { id: "a".into(), label: "Abort".into(), key: Some('a'), ..Default::default() },
    ];

    match select("Choose:", &items)?.as_str() {
        "o" => Ok(BothConflictChoice::OpenExisting),
        "n" => {
            let new_name = read_line(&format!("{}:", BOLD.paint("New worktree name")))?;
            Ok(BothConflictChoice::NewName(new_name))
        }
        _ => Ok(BothConflictChoice::Abort),
    }
}

#[derive(Default)]
pub struct SelectItem {
    pub id: String,
    pub label: String,
    pub disabled: bool,
    /// Optional single-key shortcut — pressing this key instantly selects the item.
    pub key: Option<char>,
}

/// Interactive arrow-key / tab selector. Returns the chosen item's `id`.
/// Disabled items are visible but cannot be selected.
pub fn select(title: &str, items: &[SelectItem]) -> Result<String> {
    if items.is_empty() {
        bail!("nothing to select");
    }

    let selectable_count = items.iter().filter(|i| !i.disabled).count();
    if selectable_count == 0 {
        bail!("nothing to select");
    }

    let mut stdout = io::stdout().lock();
    let mut selected: usize = items.iter().position(|i| !i.disabled).unwrap_or(0);

    terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, cursor::Hide)?;

    let result = (|| -> Result<String> {
        write!(stdout, "{}\r\n", BOLD.paint(title))?;
        render_list(&mut stdout, items, selected)?;

        loop {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    bail!("aborted");
                }

                match key.code {
                    KeyCode::Up | KeyCode::BackTab => {
                        selected = prev_selectable(items, selected);
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        selected = next_selectable(items, selected);
                    }
                    KeyCode::Enter => {
                        if !items[selected].disabled {
                            clear_list(&mut stdout, items.len())?;
                            write!(
                                stdout,
                                "{} {}\r\n",
                                BOLD.paint(title),
                                BOLD_CYAN.paint(&items[selected].label),
                            )?;
                            stdout.flush()?;
                            return Ok(items[selected].id.clone());
                        }
                    }
                    KeyCode::Esc => {
                        bail!("aborted");
                    }
                    KeyCode::Char(c) => {
                        if let Some(idx) = items.iter().position(|i| {
                            !i.disabled && i.key == Some(c.to_ascii_lowercase())
                        }) {
                            clear_list(&mut stdout, items.len())?;
                            write!(
                                stdout,
                                "{} {}\r\n",
                                BOLD.paint(title),
                                BOLD_CYAN.paint(&items[idx].label),
                            )?;
                            stdout.flush()?;
                            return Ok(items[idx].id.clone());
                        }
                    }
                    _ => {}
                }

                clear_list(&mut stdout, items.len())?;
                render_list(&mut stdout, items, selected)?;
            }
        }
    })();

    crossterm::execute!(stdout, cursor::Show)?;
    terminal::disable_raw_mode()?;
    result
}

fn next_selectable(items: &[SelectItem], current: usize) -> usize {
    let len = items.len();
    let mut idx = (current + 1) % len;
    while items[idx].disabled && idx != current {
        idx = (idx + 1) % len;
    }
    idx
}

fn prev_selectable(items: &[SelectItem], current: usize) -> usize {
    let len = items.len();
    let mut idx = if current == 0 { len - 1 } else { current - 1 };
    while items[idx].disabled && idx != current {
        idx = if idx == 0 { len - 1 } else { idx - 1 };
    }
    idx
}

fn render_list(stdout: &mut impl Write, items: &[SelectItem], selected: usize) -> Result<()> {
    for (i, item) in items.iter().enumerate() {
        let key_prefix = match item.key {
            Some(k) => format!("{} ", DIM.paint(format!("[{k}]"))),
            None => String::new(),
        };

        if item.disabled {
            write!(stdout, "    {}{}\r\n", key_prefix, DIM.paint(&item.label))?;
        } else if i == selected {
            write!(stdout, "  {} {}{}\r\n", CYAN.paint("▸"), key_prefix, BOLD.paint(&item.label))?;
        } else {
            write!(stdout, "    {}{}\r\n", key_prefix, &item.label)?;
        }
    }
    stdout.flush()?;
    Ok(())
}

fn clear_list(stdout: &mut impl Write, count: usize) -> Result<()> {
    for _ in 0..count {
        write!(stdout, "\x1b[A\x1b[2K")?;
    }
    stdout.flush()?;
    Ok(())
}
