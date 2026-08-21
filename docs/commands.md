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
| `--copy`      | Relative path to copy into new worktrees when this editor is selected (repeatable). |
| `--link`      | Relative path to symlink into new worktrees when this editor is selected (repeatable). |

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
| `--open`       | Launch the configured editor at `<worktree>/<workspace-path>`. |
| `-e`, `--editor` | With `--open`, use this **global** editor id instead of the `editor` field in `.grove.toml`. Requires `--open`. |
| `-w`, `--workspace` | Directory that contains a `.grove.toml` (resolved relative to cwd); overrides the walk from cwd to git root. |

### What it does

1. **Finds the git root** of the current repository.
2. **Loads workspace config** — walks from cwd up to the git root for `.grove.toml`, or loads from `-w`/`--workspace`. Fails if no file is found.
3. **Validates `editor` in TOML** when it is set (must match an id in `grove config editors list`). The `editor` key is optional in `.grove.toml`; if it is omitted, global + workspace `copy` / `link` rules still apply, but you need **`--editor <id>`** together with **`--open`** to launch an editor.
4. **Merges `copy` and `link` lists** from global config, the resolved editor (when `editor` is set in TOML or implied by `--editor` for `--open`), and `.grove.toml`, using the same dedupe rules as for `copy` alone. If **any relative path** appears in **both** merged lists, the command fails with an invalid-config error (before creating the worktree).
5. **Creates the worktree** at `<git-root>/../.grove__<repo-name>/<ID>` (after any conflict prompts). If a prompt offers **open existing** and you choose it, `grove` runs the same open path as `grove tend` for the workspace resolved in step 2 — **always**, whether that came from **`--workspace` / `-w`** or from the walk up from cwd — and forwards **`--editor` / `-e`** when you passed it. So `cd apps/api && grove plant feat-x` on an existing worktree opens `apps/api` inside it, and **sets it up** if that pair has never been set up (see `grove tend`). `grove plant` then exits.
6. **Saves state** for this worktree: the **workspace** path (the subdirectory under the worktree, relative to the git root — the directory that contained the chosen `.grove.toml`) is recorded as set up. State is written **before** the copy/link and `post_create` steps, so a failure part-way through does not lose the record. Any state left over from a previous worktree with the same id is **replaced**, not merged into.
7. **Creates symlinks** for merged `link` paths, then **copies** merged `copy` paths, under `<worktree-path>/<workspace-path>`, where `<workspace-path>` is the path from the git root to the directory that contains the selected `.grove.toml`. For each path, a **`copying <path>`** line is printed when that copy starts, then **`copied <path>`** on success. If a **copy** destination already exists, a **warning** is printed to stderr (`destination exists; removing before copy: …`), then it is **removed** (file, symlink, or directory tree) so the copy **replaces** it—same rule as for links below. Each symlink’s target is the **canonical** path of the corresponding file or directory in the main checkout’s workspace (the directory that contains `.grove.toml`). If a **link** destination already exists, a similar **warning** is printed (`… before link: …`), then it is **removed** so the symlink can replace it; **`linking <path>`** is printed when that link step starts, then **`linked <path>`** on success. On **Windows**, creating symlinks may require Developer Mode or an elevated shell, depending on your policy.
8. **If `--open`:** resolves the editor id as `--editor` if you passed it, otherwise `editor` from TOML, then runs that editor’s command with `$workspace` set to `<worktree-path>/<workspace-path>`. The editor is launched **after** the symlinks and copies, and **before** `post_create`, so it can start loading while those commands run.
9. **`post_create` (`.grove.toml` only):** if the workspace config has a `post_create` array of strings, each entry is run as a **shell command** from `<worktree-path>/<workspace-path>`, **sequentially**, with stdin/stdout/stderr inherited from `grove`. Before each command, a line **`run <command>`** is printed. The **first** command that exits non-zero or fails to spawn aborts `grove plant` (fail fast).

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
| Same path in merged `copy` and `link`             | Message begins with `invalid config:` and lists overlapping paths |
| `post_create` command exits non-zero             | Message begins with `post-create command failed` and includes the shell command |
| `post_create` command cannot be spawned          | Context mentions `failed to spawn post-create command` |

---

## `grove tend`

**Alias:** `grove open`

```
grove tend [ID] [-e|--editor <ID>] [-w|--workspace <DIR>] [--setup | --no-setup]
```

Reopen an existing worktree in an editor, at any of the repository’s workspaces. The **first** time a given (worktree, workspace) pair is opened, that workspace’s merged `copy` / `link` paths and its `post_create` commands are run in the worktree, exactly as `grove plant` would have done.

| Argument       | Description |
| -------------- | ----------- |
| `[ID]`         | Worktree identifier. If omitted, an interactive list of worktrees is shown. |
| `-e`, `--editor` | Use this **global** editor id for this launch instead of the `editor` field in `.grove.toml`. |
| `-w`, `--workspace` | Open this workspace directory (resolved relative to cwd) instead of picking one interactively. It need not be a workspace this worktree has been set up at before — a new one is set up on this open, just as if it had been picked from the list. |
| `--setup`      | Re-run this workspace’s `copy` / `link` and `post_create` even if it is already set up. Cannot be combined with `--no-setup`. |
| `--no-setup`   | Open without running `copy` / `link` or `post_create` for a workspace that is not set up yet. The workspace is **not** recorded as set up, so a later open without this flag still sets it up. |

### What it does

1. If `ID` is omitted, displays an interactive list of worktrees (excluding the one you are already in, if applicable).
2. **Picks the workspace.** With **`-w` / `--workspace`**, that directory is used directly and no prompt is shown. Otherwise `grove` scans the **main checkout** for directories containing a `.grove.toml`, adds any workspace already recorded for this worktree (so one whose config has since been deleted stays reachable), and sorts them **already set up first, then alphabetically**. With two or more candidates an interactive picker (`Open workspace:`) is shown, labelling each one `apps/api  (set up)` or `packages/ui  (new — will run setup)`; the git root itself is shown as `.`. A single candidate is used without prompting, and none is an error. Bare `grove tend` therefore prompts twice: worktree, then workspace.
   The scan prunes `.git`, grove’s own `.grove__*` directories, and `node_modules`, `target`, `dist`, `build`, `.next`, `vendor`, `.venv`; one of those pruned names is still **listed** when it holds a `.grove.toml` of its own, it is simply not descended into. Symlinked directories are not followed. Because the scan covers the main checkout only, a workspace that exists only on the worktree’s own branch is not offered — use `-w` for that.
3. **Decides whether to set up.** By default, only when this (worktree, workspace) pair has never been set up. **`--setup`** always sets up; **`--no-setup`** never does.
4. **Resolves the editor id:** **`--editor`** if given, otherwise the `editor` field of that workspace’s `.grove.toml` **in the main checkout** (not the copy inside the worktree). That config is read only when it is needed, so `grove tend <ID> -e <editor>` keeps working for an already-set-up workspace whose `.grove.toml` has since been deleted.
5. **Sets up the workspace**, when step 3 says so: it is first recorded as set up and state is saved — copy/link and `post_create` can fail half-way, and losing the record would re-run them on the next open — then `→ Setting up workspace <workspace> …` is printed and the merged `link` symlinks are created, followed by the merged `copy` paths, under `<worktree-path>/<workspace-path>`. Same merge rules, same warnings, and same `linking` / `copied` output as `grove plant` (steps 4 and 7 above).
6. **Launches the resolved editor** at `<worktree-path>/<workspace-path>`.
7. **Runs `post_create`** from that directory — after the editor, and only when step 5 ran. Same fail-fast rule as `grove plant`.

If the selected workspace has no `.grove.toml` in the main checkout — reachable for a workspace that was set up before its config was deleted — `grove` prints `! No .grove.toml at <path> — opening without setup.` to stderr, skips the setup, and opens the editor anyway. The workspace stays recorded, so the warning is not printed again on later opens. With **`--setup`** the same situation is a hard error instead.

There is no fallback to an arbitrary “first” global editor; you need a valid `editor` in `.grove.toml` or pass **`--editor`**.

### Errors

| Condition                          | Message (typical) |
| ---------------------------------- | ----------------- |
| Unknown worktree id                | `no worktree found with id '<ID>'` |
| No `.grove.toml` anywhere under the git root | Message begins with `no .grove.toml found under` and mentions `-w/--workspace` |
| The git root itself cannot be read while scanning | Context mentions `failed to scan <path> for .grove.toml files` |
| The `.grove.toml` for the selected workspace has no `editor` and no `--editor` was passed | `no editor in .grove.toml … or pass --editor <id>` |
| `--setup` for a workspace with no `.grove.toml` | `cannot set up workspace '<WORKSPACE>': no .grove.toml at <path>` |
| `--setup` and `--no-setup` together | Clap error: `the argument '--setup' cannot be used with '--no-setup'` |
| `-w` points at a missing directory, or at an unparseable `.grove.toml` | Context mentions `failed to load workspace config from '<DIR>'` |
| The workspace is outside the repository and its worktrees | `workspace directory <path> is not inside the git repository or any of its worktrees` |
| Same path in merged `copy` and `link` (while setting up) | Message begins with `invalid config:` and lists overlapping paths |
| Editor not found in global config  | `editor '<ID>' not found` |
| Editor fails to launch             | `failed to launch editor '<command>': <OS error>` |
| `post_create` command exits non-zero | Message begins with `post-create command failed` and includes the shell command |

---

## `grove trees`

**Aliases:** `grove ls`, `grove list`

```
grove trees [--details]
```

List all worktrees and their status. The main checkout is not listed.

### Output

One line per worktree:

```
  feat-a ✔ clean  wip/feat-a
▸ feat-b [uncommitted, ↑2]  wip/feat-b
```

- **Identifier** — from the state file; for a worktree grove does not track, the directory name is shown instead (or `?`), followed by `(untracked)`. A worktree created directly with `git worktree add` is matched by its directory name, so it is *not* marked untracked once grove has opened it.
- **Status** — `✔ clean`, or a bracketed list of `uncommitted`, `not pushed`, `↑<ahead>`, `↓<behind>` (empty for a bare worktree).
- **Branch** name (or `(detached)` / `(bare)`).

With **`--details`**, each worktree is printed over several lines instead, separated by a blank line, adding:

- **`path:`** — the worktree directory on disk.
- **`branch:`** — the same branch name.
- **`workspaces:`** — the workspace subdirectories grove has set up in that worktree, comma-separated, with the git root itself shown as `.` (for example `workspaces: ., apps/api`). Omitted when there are none.

Worktrees are listed newest first; the current worktree (if you are inside one) is marked with `▸` and listed first.

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

- Worktree ids for **`grove tend`** / **`grove open`** and **`grove cut`** (and that subcommand’s own flags — including `--setup` / `--no-setup` on `grove tend` — once you type a leading `-`).
- **Global editor ids** (with optional descriptions) after **`-e` / `--editor`** on **`grove plant`** / **`grove new`** / **`grove create`** and **`grove tend`** / **`grove open`**.

Other flags and subcommands are completed via `clap_complete` dynamic integration.
