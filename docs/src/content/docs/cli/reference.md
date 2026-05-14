---
title: CLI reference
description: Every evault subcommand, its flags, and a worked example.
---

`evault` is a single binary with a default action (launch the TUI) and a set of non-interactive subcommands for scripting. Every subcommand reads from the same persistent backend the TUI uses — the metadata DB plus the OS keyring.

## Global flags

| Flag | Effect |
|---|---|
| `--demo` | Use an ephemeral in-memory backend seeded with 10 sample variables. Never touches the keyring or DB. |
| `--ephemeral` | Use an empty in-memory backend. |
| `--version` | Print the binary version and exit. |
| `--help` | Print usage. |

`--demo` and `--ephemeral` are mutually exclusive. Neither applies to `evault reset` (which by design only operates on the persistent backend).

## `evault` — launch the TUI

```bash
evault              # against the real persistent registry
evault --demo       # against the seeded ephemeral backend
evault --ephemeral  # against an empty ephemeral backend
```

See [TUI keybindings](/hide-env-keys/tui/keybindings/) for the keymap.

## `evault ls` — list variables

```bash
evault ls
```

Prints every variable with its group, kind, value length, linked-project count, and last-update timestamp. Sorted by name.

## `evault add NAME` — create

```bash
evault add API_KEY --secret
evault add NODE_ENV --group user
evault add CUSTOM_VAR --group ops --secret
```

| Flag | Default | Effect |
|---|---|---|
| `--secret` | off | Store in the OS keyring (vs the metadata DB). |
| `--group <name>` | `user` | `user`, `system`, `project`, or any custom string. |

The value is prompted interactively. For `--secret`, input is read with no echo (`rpassword`). The variable's name must satisfy `[A-Za-z_][A-Za-z0-9_]{0,63}`.

## `evault rm NAME` — delete

```bash
evault rm API_KEY        # interactive y/N
evault rm API_KEY -y     # no prompt
```

`-y` / `--yes` skips the confirmation. Required for scripts and CI.

## `evault link NAME --project PATH` — bind to a project

```bash
evault link API_KEY --project ./my-app
evault link DATABASE_URL --project ./api --profile staging --alias DB_URL
```

| Flag | Default | Effect |
|---|---|---|
| `--project <path>` | (required) | Project root; canonicalised before write. Created if missing. |
| `--profile <name>` | `default` | Profile under which to write the binding. |
| `--alias <key>` | (variable name) | Expose the variable under a different key in the project. |

Writes (or updates) `<project>/evault.toml`. Idempotent: re-running with the same flags is a no-op.

## `evault gen --project PATH` — materialize `.env`

```bash
evault gen --project ./my-app
evault gen --project ./api --profile staging
```

Resolves every binding for the chosen profile and writes `<project>/.env`. Properties:

- **Atomic** — temp file + `fs::rename`. A crash mid-write never leaves a half-written `.env` visible.
- **Permissioned** — mode `0o600` on Unix; ACL-inherited on Windows.
- **`.gitignore`-managed** — sibling `.gitignore` is updated automatically.
- **Injection-rejecting** — CRLF and NUL bytes in values are rejected.

## `evault run --project PATH -- CMD [ARGS...]` — spawn with env injected

```bash
evault run --project ./my-app -- npm start
evault run --project ./api --profile staging -- ./serve --port 8080
evault run --project ./py -- python manage.py runserver
```

Resolves bindings for the chosen profile and spawns the child with the overlay applied. **No `.env` is written.** The child inherits stdio so interactive programs work normally. The child's exit code is forwarded as the binary's exit code.

The `EVAULT_*` prefix is stripped from the parent environment before applying the overlay, so internal evault config never leaks into untrusted children.

## `evault scan PATH` — find orphan / unused variables

```bash
evault scan ./my-app
```

Walks the directory tree (skipping common build / cache dirs), classifies every variable reference into:

- **ORPHAN** — referenced in code but absent from the registry.
- **UNUSED** — present in the registry but not referenced anywhere in the scanned tree.
- **REFERENCED** — present in both.

Recognised languages: JavaScript / TypeScript, Python, Rust, Go, Shell. See the [`evault-scanner-regex`](https://crates.io/crates/evault-scanner-regex) crate for the detection patterns.

## `evault import PATH` — bulk-import a `.env`

```bash
evault import ./.env --secret
evault import ./.env --group project
```

| Flag | Default | Effect |
|---|---|---|
| `--secret` | off | Store imported values in the keyring. |
| `--group <name>` | `user` | Group label for imported variables. |

Non-destructive: existing variable names in the registry are skipped, not overwritten.

## `evault export` — dump the registry

```bash
evault export
evault export --mask
```

Writes `KEY=VALUE` lines to stdout, one per managed variable. `--mask` replaces secret values with `*****` so the output is safe to paste into a chat or issue.

## `evault reset` — recovery

```bash
evault reset
evault reset -y
```

Wipes the metadata DB and the master-key keyring entry. Required when a binary upgrade leaves an incompatible DB on disk. See [Recovery](/hide-env-keys/cli/recovery/) for the symptom and procedure.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | Runtime failure (backend open, validation, IO). The message is on stderr. |
| `2` | CLI misuse / unimplemented feature. Reserved for future stubbed subcommands. |
| Otherwise | `evault run` forwards the child's exit code. |
