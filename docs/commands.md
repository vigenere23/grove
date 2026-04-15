# Command Reference

## `grove config editors list`

List all editors defined in the global config.

**Output:**

```
code:
  name:    VSCode
  command: code $workspace
cursor:
  name:    Cursor
  command: cursor $workspace
```

If no editors are configured, prints a hint to add one.

---

## `grove config editors add`

```
grove config editors add <ID> --name <NAME> --command <COMMAND>
```

Register a new editor in the global config (`~/.config/grove/config.toml`).

| Argument      | Description                                                        |
| ------------- | ------------------------------------------------------------------ |
| `<ID>`        | Unique identifier for the editor (e.g. `code`, `cursor`, `nvim`)   |
| `--name`      | Human-readable display name                                        |
| `--command`   | Shell command to launch the editor. Use `$workspace` as a placeholder for the workspace path. |

**Example:**

```bash
grove config editors add code --name "VSCode" --command "code \$workspace"
```

**Errors:**

| Condition                     | Message                        |
| ----------------------------- | ------------------------------ |
| An editor with `<ID>` exists  | `editor '<ID>' already exists` |

---

## `grove config editors remove`

```
grove config editors remove <ID>
```

Remove an editor from the global config.

| Argument | Description                          |
| -------- | ------------------------------------ |
| `<ID>`   | Identifier of the editor to remove   |

**Errors:**

| Condition                          | Message                       |
| ---------------------------------- | ----------------------------- |
| No editor with `<ID>` exists       | `editor '<ID>' not found`     |

---

## `grove plant`

**Aliases:** `grove new`, `grove create`

```
grove plant <ID> [--from <BRANCH>] [--branch <BRANCH>] [--open] [-e|--editor <ID>] [-w|--workspace <DIR>]
```

Create a git worktree using the nearest `.grove.toml` between the current directory and the git root (or the one under `-w` / `--workspace`). The editor is **not** launched unless you pass **`--open`**.

| Argument       | Description |
| -------------- | ----------- |
| `<ID>`         | Worktree identifier (directory name under `.grove__<repo>/` and default branch `wip/<ID>`). *Required.* |
| `--from`       | Existing branch to base the new worktree branch on. Default: current branch. |
| `--branch`     | Branch name to create or attach in the worktree. Default: `wip/<ID>`. |
| `--open`       | After creating the worktree and copying files, launch an editor at `<worktree>/<workspace-path>`. |
| `-e`, `--editor` | With `--open`, use this **global** editor id instead of the `editor` field in `.grove.toml`. Requires `--open`. |
| `-w`, `--workspace` | Directory that contains a `.grove.toml` (resolved relative to cwd); overrides the walk from cwd to git root. |

### What it does

1. **Finds the git root** of the current repository.
2. **Loads workspace config** — walks from cwd up to the git root for `.grove.toml`, or loads from `-w`/`--workspace`. Fails if no file is found.
3. **Validates `editor` in TOML** when it is set (must match an id in `grove config editors list`). The `editor` key is optional in `.grove.toml`; if it is omitted, global + workspace `copy` rules still apply, but you need **`--editor <id>`** together with **`--open`** to launch an editor.
4. **Creates the worktree** at `<git-root>/../.grove__<repo-name>/<ID>` (after any conflict prompts).
5. **Copies paths** from the global config and from the resolved editor’s `copy` list, plus any `copy` entries in `.grove.toml`, into `<worktree-path>/<workspace-path>`, where `<workspace-path>` is the path from the git root to the directory that contains the selected `.grove.toml`.
6. **If `--open`:** resolves the editor id as `editor` from TOML, or `--editor` if given, then runs that editor’s command with `$workspace` set to the directory from step 5.
7. **Saves state** for this worktree: only the **workspace** path (the subdirectory under the worktree, relative to the git root — the directory that contained the chosen `.grove.toml`).

### Example

Given this layout:

```
/home/project/              <- git root
/home/project/.grove.toml   <- editor = "cursor"
/home/project/moduleA/
/home/project/moduleA/.grove.toml  <- editor = "code"
```

Running from `/home/project/moduleA`:

```bash
grove plant featureX --from main --branch feat/test-123/new-command --open
```

Result:

1. Worktree created at `/home/.grove__project/featureX` on branch `feat/test-123/new-command` from `main`.
2. Editor `code` (from `moduleA/.grove.toml`) launched at `/home/.grove__project/featureX/moduleA`.

To open with a different global editor without changing TOML:

```bash
grove plant featureX --open --editor cursor
```

### Errors

| Condition                                          | Message (typical) |
| -------------------------------------------------- | ----------------- |
| Not inside a git repository                        | `failed to find git repository root` (or similar from `git`) |
| HEAD is detached and `--from` not provided         | `could not determine the current branch — HEAD may be detached` |
| Worktree / branch conflict                         | Interactive prompt or `aborted` |
| Git worktree command fails                         | `failed to create git worktree` with underlying `git` error |
| `.grove.toml` references an unknown editor ID    | `editor '<ID>' not found` |
| No `.grove.toml` found between cwd and git root    | `no .grove.toml found between <cwd> and git root <root>` |
| `--open` but no TOML `editor` and no `--editor`   | Message begins with `cannot use --open: no` and mentions `grove config editors list` and `--editor <id>`. |
| `--editor` without `--open` on `plant`            | Clap error: `--editor` requires `--open` |
| Editor command is empty                            | `editor command is empty` |
| Editor binary cannot be executed                   | `failed to launch editor '<command>': <OS error>` |

---

## `grove tend`

**Alias:** `grove open`

```
grove tend [ID] [-e|--editor <ID>] [-w|--workspace <DIR>]
```

Reopen an existing worktree in an editor.

| Argument       | Description |
| -------------- | ----------- |
| `[ID]`         | Worktree identifier. If omitted, an interactive list of worktrees is shown. |
| `-e`, `--editor` | Use this **global** editor id for this launch instead of the `editor` field in `.grove.toml`. |
| `-w`, `--workspace` | Load `.grove.toml` from this directory (relative to cwd). Updates the **saved workspace path** for this worktree in state (editor is still read from that TOML or from `--editor`). |

### What it does

1. If `ID` is omitted, displays an interactive list of worktrees (excluding the one you are already in, if applicable).
2. Resolves **workspace path** and **editor id**:
   - With **`-w` / `--workspace`:** loads that `.grove.toml`; the workspace subdirectory is derived from the config file’s location. The editor id comes from the TOML’s `editor` field or from **`--editor`**. Persists **only** the workspace path in state for this worktree id.
   - Otherwise, if grove has **saved state** for this id, uses the **stored workspace path** only; the editor id always comes from `.grove.toml` inside the worktree (walk from cwd or the worktree root) or from **`--editor`**.
   - If there is **no** state entry, walks the same way inside the worktree for `.grove.toml`, then may save **only** the workspace path to state after a successful open.
3. Launches the resolved editor at `<worktree-path>/<workspace-path>`.

There is no fallback to an arbitrary “first” global editor; you need a valid `editor` in `.grove.toml` or pass **`--editor`**.

### Errors

| Condition                          | Message (typical) |
| ---------------------------------- | ----------------- |
| Unknown worktree id                | `no worktree found with id '<ID>'` |
| No `.grove.toml` with enough info  | `no .grove.toml found between …` or `no editor in .grove.toml … or pass --editor <id>` |
| Editor not found in global config  | `editor '<ID>' not found` |
| Editor fails to launch             | `failed to launch editor '<command>': <OS error>` |

---

## `grove trees`

**Aliases:** `grove ls`, `grove list`

List all worktrees and their status.

### Output

For each worktree, displays:

- **Identifier** (or `?` if not tracked in state)
- **Dirty/clean status** (`[dirty]` or `[clean]`)
- **Path** on disk
- **Branch** name (or `(detached)` / `(bare)`)
- **Workspace** subdirectory (if tracked)

The current worktree (if you are inside one) is marked with `▸` and listed first.

---

## `grove cut`

**Aliases:** `grove rm`, `grove remove`

```
grove cut <ID> [--force] [--branch <MODE>]
grove cut --all [--force] [--branch <MODE>]
```

Remove a worktree and clean up state.

| Argument        | Description                                              | Default    |
| --------------- | -------------------------------------------------------- | ---------- |
| `<ID>`          | Worktree identifier (omit when using `--all`)            | *required* |
| `--force`       | Remove even if the worktree has uncommitted changes      | `false`    |
| `--all`         | Remove all tracked worktrees                             | `false`    |
| `--branch`      | Branch cleanup mode                                      | *(none)*   |

### Branch cleanup modes

| Value             | Description                              |
| ----------------- | ---------------------------------------- |
| `delete`          | Delete the local branch after removal    |
| `delete-remote`   | Delete both local and remote branches    |

### What it does

1. **Checks for uncommitted changes** — refuses to remove a dirty worktree unless `--force` is passed.
2. **Resolves the branch** associated with the worktree (for optional cleanup).
3. **Removes the git worktree** (`git worktree remove`).
4. **Optionally deletes the branch** (local and/or remote) based on `--branch`.
5. **Removes the entry** from the state file.

If the worktree directory is already gone (e.g. manually deleted), the state entry is cleaned up silently.

With `--all`, only clean worktrees are removed; dirty worktrees are skipped (printed with a "skipped (dirty)" notice). Use `--force` to include dirty worktrees. Errors on individual worktrees are collected and reported at the end; successful removals still take effect.

### Examples

```bash
# Remove a clean worktree
grove cut my-feature

# Force-remove a dirty worktree
grove cut my-feature --force

# Remove and delete the local branch
grove cut my-feature --branch delete

# Remove and delete both local and remote branches
grove cut my-feature --branch delete-remote

# Remove all worktrees
grove cut --all

# Force-remove all worktrees and delete all branches
grove cut --all --force --branch delete
```

### Errors

| Condition                                    | Message                                                                        |
| -------------------------------------------- | ------------------------------------------------------------------------------ |
| Neither `<ID>` nor `--all` provided          | `worktree id is required (or use --all)`                                       |
| Both `<ID>` and `--all` provided             | `cannot specify both an id and --all`                                          |
| Worktree has uncommitted changes (no --force)| `worktree '<ID>' has uncommitted changes — commit/stash them first, or use --force` |
| Git worktree remove fails                    | `git command failed: <git stderr>`                                             |

---

## Shell completions

When the binary is sourced with `COMPLETE=zsh`, `COMPLETE=bash`, or `COMPLETE=fish` (see the project `README.md`), dynamic completions include:

- Worktree ids for **`grove tend`** / **`grove open`** and **`grove cut`** (and related flags where implemented).
- **Global editor ids** (with optional descriptions) after **`-e` / `--editor`** on **`grove plant`** / **`grove new`** / **`grove create`** and **`grove tend`** / **`grove open`**.

Other flags and subcommands are completed via `clap_complete` dynamic integration.
