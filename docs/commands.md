# Command Reference

## `parai config editors list`

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

## `parai config editors add`

```
parai config editors add <ID> --name <NAME> --command <COMMAND>
```

Register a new editor in the global config (`~/.config/parai/config.toml`).

| Argument      | Description                                                        |
| ------------- | ------------------------------------------------------------------ |
| `<ID>`        | Unique identifier for the editor (e.g. `code`, `cursor`, `nvim`)   |
| `--name`      | Human-readable display name                                        |
| `--command`   | Shell command to launch the editor. Use `$workspace` as a placeholder for the workspace path. |

**Example:**

```bash
parai config editors add code --name "VSCode" --command "code \$workspace"
```

**Errors:**

| Condition                     | Message                        |
| ----------------------------- | ------------------------------ |
| An editor with `<ID>` exists  | `editor '<ID>' already exists` |

---

## `parai config editors remove`

```
parai config editors remove <ID>
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

## `parai start`

```
parai start <ID> [--from <BRANCH>] [--branch <BRANCH>]
```

Create a git worktree and open the configured editor.

| Argument     | Description                                           | Default              |
| ------------ | ----------------------------------------------------- | -------------------- |
| `<ID>`       | Worktree identifier (used in the directory name and default branch) | *required* |
| `--from`     | Existing branch to base the worktree on               | Current branch       |
| `--branch`   | New branch name to create in the worktree             | `wip/<ID>`           |

### What it does

1. **Finds the git root** of the current repository.
2. **Creates a worktree** at `<git-root>/../.<repo-name>_worktrees/<ID>`.
3. **Resolves the editor:**
   - Walks from the current directory up to the git root looking for `.parai.toml` files.
   - Uses the `editor` field from the most specific (closest) config found.
   - If no `.parai.toml` exists, falls back to the first editor in the global config.
4. **Opens the editor** at `<worktree-path>/<workspace-path>`, where `<workspace-path>` is the relative path from the git root to the directory containing the selected `.parai.toml`.

### Example

Given this layout:

```
/home/project/              <- git root
/home/project/.parai.toml   <- editor = "cursor"
/home/project/moduleA/
/home/project/moduleA/.parai.toml  <- editor = "code"
```

Running from `/home/project/moduleA`:

```bash
parai start featureX --from main --branch feat/test-123/new-command
```

Result:

1. Worktree created at `/home/.project_worktrees/featureX` on branch `feat/test-123/new-command` from `main`.
2. Editor `code` (from `moduleA/.parai.toml`) launched at `/home/.project_worktrees/featureX/moduleA`.

### Errors

| Condition                                          | Message                                                                 |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| Not inside a git repository                        | `not inside a git repository (or any parent up to the filesystem root)` |
| HEAD is detached and `--from` not provided         | `could not determine the current branch — HEAD may be detached`         |
| Worktree directory already exists                  | `worktree already exists at <path>`                                     |
| Git worktree command fails                         | `git command failed: <git stderr>`                                      |
| `.parai.toml` references an unknown editor ID      | `editor '<ID>' not found`                                               |
| No `.parai.toml` found and no editors in global config | `no editors configured — add one with parai config editors add`     |
| Editor command is empty                            | `editor command is empty`                                               |
| Editor binary cannot be executed                   | `failed to launch editor '<command>': <OS error>`                       |
