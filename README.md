# grove

Manage parallel workspaces with git worktrees.

`grove` uses [git worktrees](https://git-scm.com/docs/git-worktree) to give each workspace its own isolated copy of the repository. You can optionally open your editor in the right subdirectory in the same flow (`grove plant … --open`).

## Requirements

- **Git** >= 2.20
- An editor launchable from the command line (e.g. `code`, `cursor`, `nvim`)

## Installation

<!-- TODO: add install instructions once published -->

Coming soon.

## Quick start

```bash
# 1. Register an editor
grove config editors add code --name "VSCode" --command "code \$workspace"

# 2. Add a workspace config (.grove.toml) anywhere between your cwd and the
#    git root. `editor` is optional unless you use `grove plant --open` without
#    `--editor` (see docs/commands.md).
echo 'editor = "code"' > .grove.toml

# 3. Spin up a worktree (add --open to launch the editor after create)
grove plant my-feature --from main --open

# 4. When you're done, remove the worktree
grove cut my-feature
```

This creates a worktree at `<git-root>/../.grove__<repo>/my-feature` on a new branch `wip/my-feature`. With `--open`, it launches the editor from your global config (from `.grove.toml`’s `editor`, or from `--editor <id>` together with `--open`). You can also use `grove new` or `grove create` as aliases for `grove plant`, and `grove rm` or `grove remove` for `grove cut`.

## Configuration

### Global config — `~/.config/grove/config.toml`

Stores editor definitions. Created automatically on first use. Respects `$XDG_CONFIG_HOME` if set (defaults to `~/.config`).

```toml
[editors]

[editors.code]
name = "VSCode"
command = "code $workspace"

[editors.cursor]
name = "Cursor"
command = "cursor $workspace"
```

The `$workspace` placeholder is replaced with the resolved workspace path at launch time.

At the top level of the same file you may set **`copy`** and **`link`** arrays (paths relative to the workspace directory) that apply for every editor; per-editor tables may also include **`copy`** and **`link`**. See [docs/commands.md](docs/commands.md) for how `grove plant` merges these with `.grove.toml`.

### Workspace config — `.grove.toml`

Place a `.grove.toml` file in any directory within a git repository to control the **workspace directory** (the directory containing the file defines the subdirectory used inside each new worktree). Optional fields:

- **`editor`** — id of an editor defined in the global config. Used for merged `copy` / `link` paths and for `grove plant --open` / `grove tend` when you do not pass `--editor`. Omit it if you only need `copy` / `link` rules or never open an editor from that config.
- **`copy`** — extra relative paths to copy into the worktree (see command reference). If something already exists at that path in the new worktree, a warning is printed and it is removed first so the copy replaces it (same as `link`).
- **`link`** — extra relative paths to symlink into the worktree; each link points at the same path under your main checkout’s workspace directory so edits stay shared (see command reference). If something already exists at that path, a warning is printed and it is removed before linking. A path must not appear in both `copy` and `link` after merging with global config.
- **`post_create`** — optional list of shell commands to run from the new worktree’s workspace directory **after** copy/link and **after** `grove plant --open` launches the editor (when `--open` is used). Not supported in global config—workspace only. Commands run in order; output goes to your terminal; the first failure aborts `grove plant`.

```toml
editor = "code"
# copy = ["scripts/dev.sh"]
# link = [".vscode/settings.json"]
# post_create = ["npm ci"]
```

When multiple `.grove.toml` files exist between your current directory and the git root, the most specific one (closest to your current directory) takes priority.

A `.grove.toml` is required when creating a new worktree. If none is found between your current directory and the git root, `grove plant` will return an error.

Grove’s per-repo state file (under `.grove__<repo>/`) records **only** the workspace subdirectory for each worktree id so `grove tend` can open the same folder again. It does **not** store which editor to use; that always comes from `.grove.toml` or from `--editor` when you pass it.

## Command reference

See [docs/commands.md](docs/commands.md) for the full command reference.

## Shell completions

`grove` supports tab completions for all commands, flags, and arguments — including dynamic completion of worktree IDs for `grove tend` and `grove cut`, and **global editor ids** after `-e` / `--editor` on `grove plant` / `grove tend`.

Add the appropriate line to your shell profile:

```bash
# zsh (~/.zshrc)
source <(COMPLETE=zsh grove)

# bash (~/.bashrc)
source <(COMPLETE=bash grove)

# fish (~/.config/fish/config.fish)
COMPLETE=fish grove | source
```

Then restart your shell (or `source` the profile) to enable completions.

## Building from source

### Prerequisites

- **Rust** >= 1.85

### Build and install locally

```bash
git clone <repo-url> && cd grove
./install.sh
```

This builds a release binary and symlinks it to `~/.local/bin/grove`. The script warns you if `~/.local/bin` is not in your `PATH`.

### Build only

```bash
cargo build --release
```

The binary is produced at `target/release/grove`.

### Run without installing

```bash
cargo run -- <command> [args]
```

## License

MIT
