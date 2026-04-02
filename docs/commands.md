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
grove plant <ID> [--from <BRANCH>] [--branch <BRANCH>]
```

Create a git worktree and open the configured editor.

| Argument     | Description                                           | Default              |
| ------------ | ----------------------------------------------------- | -------------------- |
| `<ID>`       | Worktree identifier (used in the directory name and default branch) | *required* |
| `--from`     | Existing branch to base the worktree on               | Current branch       |
| `--branch`   | New branch name to create in the worktree             | `wip/<ID>`           |

### What it does

1. **Finds the git root** of the current repository.
2. **Creates a worktree** at `<git-root>/../.grove__<repo-name>/<ID>`.
3. **Resolves the editor:**
   - Walks from the current directory up to the git root looking for `.grove.toml` files.
   - Uses the `editor` field from the most specific (closest) config found.
   - If no `.grove.toml` exists, returns an error.
4. **Opens the editor** at `<worktree-path>/<workspace-path>`, where `<workspace-path>` is the relative path from the git root to the directory containing the selected `.grove.toml`.

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
grove plant featureX --from main --branch feat/test-123/new-command
```

Result:

1. Worktree created at `/home/.grove__project/featureX` on branch `feat/test-123/new-command` from `main`.
2. Editor `code` (from `moduleA/.grove.toml`) launched at `/home/.grove__project/featureX/moduleA`.

### Errors

| Condition                                          | Message                                                                 |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| Not inside a git repository                        | `not inside a git repository (or any parent up to the filesystem root)` |
| HEAD is detached and `--from` not provided         | `could not determine the current branch — HEAD may be detached`         |
| Worktree directory already exists                  | `worktree already exists at <path>`                                     |
| Git worktree command fails                         | `git command failed: <git stderr>`                                      |
| `.grove.toml` references an unknown editor ID      | `editor '<ID>' not found`                                               |
| No `.grove.toml` found between cwd and git root        | `no .grove.toml found between <cwd> and git root <root>`           |
| Editor command is empty                            | `editor command is empty`                                               |
| Editor binary cannot be executed                   | `failed to launch editor '<command>': <OS error>`                       |

---

## `grove tend`

**Alias:** `grove open`

```
grove tend [ID]
```

Reopen an existing worktree in the configured editor.

| Argument | Description            |
| -------- | ---------------------- |
| `[ID]`   | Worktree identifier (optional) |

### What it does

1. If `ID` is omitted, displays an interactive list of worktrees and lets you pick one with arrow keys / tab + enter.
2. Looks up the worktree in the state file.
3. Verifies the worktree directory still exists.
4. Opens the editor at the stored workspace path.

### Errors

| Condition                          | Message                                                                        |
| ---------------------------------- | ------------------------------------------------------------------------------ |
| Worktree not found in state        | `worktree '<ID>' not found in state`                                           |
| Worktree directory doesn't exist   | `worktree directory does not exist: <path>` (may have been removed or pruned)  |
| Editor not found in global config  | `editor '<ID>' not found`                                                      |
| Editor fails to launch             | `failed to launch editor '<command>': <OS error>`                              |

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

The current worktree (if you're inside one) is marked with `*` and listed first.

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
