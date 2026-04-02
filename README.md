# grove

Manage parallel workspaces with git worktrees.

`grove` uses [git worktrees](https://git-scm.com/docs/git-worktree) to give each workspace its own isolated copy of the repository, then opens your configured editor in the right directory — all in a single command.

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

# 2. Add a workspace config so grove knows which editor to use
#    and which subdirectory to open. Place this anywhere between your
#    current directory and the git root.
echo 'editor = "code"' > .grove.toml

# 3. Spin up a worktree and open the editor
grove plant my-feature --from main

# 4. When you're done, remove the worktree
grove cut my-feature
```

This creates a worktree at `<git-root>/../.grove__<repo>/my-feature` on a new branch `wip/my-feature`, then opens VSCode pointing at the correct subdirectory. You can also use `grove new` or `grove create` as aliases for `grove plant`, and `grove rm` or `grove remove` for `grove cut`.

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

### Workspace config — `.grove.toml`

Place a `.grove.toml` file in any directory within a git repository to control which editor is used and which subdirectory is opened in the worktree.

```toml
editor = "code"
```

When multiple `.grove.toml` files exist between your current directory and the git root, the most specific one (closest to your current directory) takes priority.

A `.grove.toml` is required when creating a new worktree. If none is found between your current directory and the git root, `grove plant` will return an error.

## Command reference

See [docs/commands.md](docs/commands.md) for the full command reference.

## Shell completions

`grove` supports tab completions for all commands, flags, and arguments — including dynamic completion of worktree IDs for `grove tend` and `grove cut`.

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
