---
title: Run a command in a project
description: How the TUI's R keybinding spawns a child process with the resolved environment overlay injected — no .env on disk.
---

The `R` keybinding (uppercase — lowercase `r` is Refresh) launches a child process with the project's resolved environment overlay applied. No `.env` file is written. This is the TUI equivalent of `evault run --project PATH -- CMD [ARGS...]` from the shell.

## The modal

Press `R` from the dashboard or detail view to open the run-in-project modal:

```
┌─ run in project ──────────────────────────────┐
│                                                │
│  Path:     ./my-app                            │
│  Profile:  default                             │
│  Command:  npm start                           │
│                                                │
│  Tab cycle · Enter run · Esc cancel            │
└────────────────────────────────────────────────┘
```

Fields, in cycle order:

| Field | Notes |
|---|---|
| `Path` | Filesystem path to the project root. Must contain an `evault.toml`. |
| `Profile` | Profile name. Empty → `default`. |
| `Command` | Program + args, tokenised on whitespace at submit time. |

The `Command` field is **whitespace-split**. Quoted arguments and shell pipes are not supported in the modal — for complex shell quoting needs, exit the TUI and use `evault run` from the shell instead.

## What happens on submit

When you press `Enter`, the runtime:

1. **Tears the alternate screen down** via `ratatui::try_restore()`. The TUI exits raw mode and leaves the alternate screen buffer. From here forward, your terminal is in its normal state — exactly as it would be if you had launched the program from a shell prompt.
2. **Loads `<path>/evault.toml`** and resolves every binding under the chosen profile. Secret bindings hit the OS keyring; inline literals come straight from the manifest text.
3. **Spawns the child synchronously** with `std::process::Command`. The child inherits stdin, stdout, and stderr — so REPLs, build watchers, interactive prompts, anything that needs a TTY, all work normally.
4. **Re-enters raw mode + alternate screen** when the child exits, via `ratatui::try_init()`.
5. **Refreshes the dashboard** from the backend.
6. **Shows a toast** with the exit code, e.g. `ran "npm start" (exit 0)`.

## Failure modes

The runtime catches three classes of failure:

- **Backend panic** — caught with `catch_unwind`, surfaced as an error modal titled "run failed" with a hint to file a bug.
- **Backend `Err`** — surfaced as an error modal with a contextual hint derived from the message:
  - "manifest not found" → suggests linking a variable first to scaffold `evault.toml`.
  - "program not found" → suggests checking PATH or using an absolute program path.
  - "permission denied" → platform-specific hint (`chmod +x` / `Unblock-File`).
- **Terminal restore failure** — refuses to spawn at all, surfaces an error modal explaining the terminal could not be restored cleanly.

After dismissing the error modal with `Esc` or `Enter`, you are back on the dashboard with the registry refreshed.

## Why no `.env` on disk

The TUI's `R` flow and the CLI's `evault run` subcommand share the same `run_helper` under the hood. The resolved environment overlay is built into a `BTreeMap<String, String>` in memory and passed directly to `Command::env(key, value)` before `spawn()`. The bytes never touch the filesystem.

If a process snooping the user's filesystem could read the resolved values, that would defeat the point of pulling secrets from the OS keyring in the first place. The runtime takes the longer path (terminal save/restore around a blocking spawn) precisely to avoid that.

## Comparison to `evault gen`

| | `evault gen` / TUI link with materialize | `evault run` / TUI run-in-project |
|---|---|---|
| Writes `.env` to disk | Yes (atomic; gitignored) | No |
| Subsequent processes pick it up | Yes — any tool that reads `.env` | No — the env applies only to the spawned child |
| Risk surface | Disk read by anything in the project tree | Process tree only |
| Use case | Editor / IDE plugin that auto-loads `.env`; `docker compose` workflows | Single-command runs, CI scripts, ad-hoc dev tasks |

Pick `gen` when an external tool needs the file; pick `run` when you only need the env for one command.
