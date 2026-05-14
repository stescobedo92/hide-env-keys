---
title: Modals & error UX
description: How the TUI surfaces forms, confirms, decrypted values, and action failures.
---

The TUI uses a layered modal system. Only one modal is focused at a time, and the focused modal swallows almost every keypress except its own controls so you cannot accidentally trigger a side effect while reading or filling out a form.

## Layer order (top to bottom)

When multiple overlays could match a keypress, the order of resolution is:

1. **Error modal** — must be acknowledged before anything else accepts input.
2. **Confirm modal** — delete confirmations and similar destructive prompts.
3. **View-value modal** — read-only secret reveal.
4. **Link form** — modal for `l` (link to project).
5. **Run form** — modal for `R` (run in project).
6. **Editor form** — modal for `n` (new var) and `e` (edit value).
7. **Fuzzy filter input** — `Ctrl+F`.
8. **Help overlay** — `?`.
9. The **dashboard** or **detail** view (normal keys apply).

`Esc` walks this stack top-down: the topmost overlay closes first. If nothing is open, `Esc` returns from Detail to Dashboard, or quits if you are already at the root.

## Editor form (new / edit variable)

Triggered by `n` (new) or `e` (edit value) on the selected row.

Fields, in cycle order:

| Field | Type | Notes |
|---|---|---|
| `Name` | Text | Validated against `[A-Za-z_][A-Za-z0-9_]{0,63}`. Read-only in edit mode. |
| `Group` | Cycler | `user`, `system`, `project`. Use `←` / `→` or `Space`. |
| `Kind` | Cycler | `Secret` (keyring) or `Plain` (metadata DB). Use `←` / `→` or `Space`. |
| `Value` | Text | Masked as `*` by default for secret kind; `Ctrl+S` toggles. |

Controls: `Tab` / `Shift+Tab` cycle fields, `Enter` submits, `Esc` cancels. On a validation failure the form stays open and the error surfaces in an error modal with a plain-English hint.

## Confirm modal (delete)

Triggered by `d`. The modal asks "delete `<NAME>`? [y/N]" with bold yes / dim no. `y` confirms, `n` / `Esc` cancels. The actual delete call happens at the runtime boundary (the layer that owns the backend) after the user confirms — the state machine never deletes on its own.

## View-value modal

Triggered by `v`. The runtime fetches the decrypted value from the backend, then `AppState::show_value_modal` opens the modal. By default the value is masked; `Ctrl+S` toggles. `Esc` closes.

The runtime never holds the decrypted value beyond what the modal needs — it lives in a `secrecy::SecretString` and is zeroized on drop.

## Link form

Triggered by `l`. Fields: project `Path`, `Profile` (defaults to `default`), and `Materialize` (toggle: also write `.env` after linking). `Tab` cycles, `Enter` submits, `Esc` cancels.

## Run form

Triggered by `R` (uppercase). See [Run in a project](/hide-env-keys/tui/run-in-project/) for the full flow including terminal restore semantics.

## Error modal

Whenever an action fails (validation error, missing manifest, spawn error, backend panic), the result is surfaced in a centered error modal rather than a toast:

- **Title** — short label, e.g. `create failed`, `link failed`, `run failed`.
- **Message** — the raw backend error.
- **Hint** — a plain-English explanation of what went wrong and how to recover. The hint is computed by pattern-matching on the message, with a fallback to "no hint" when none of the recognised patterns match.

Only `Enter`, `Esc`, and `Ctrl+C` are recognised while the error modal is focused. The modal swallows everything else so you cannot accidentally trigger a new action while still reading the error.

### Example hints

- **Invalid name**: explains the `[A-Za-z_][A-Za-z0-9_]{0,63}` rules with concrete examples (`API_KEY`, `DATABASE_URL`, `my_token`).
- **Duplicate name**: suggests pressing `e` on the existing row to update its value.
- **Manifest not found** (run): explains that the project needs an `evault.toml` and offers `l` (link) as the fix.
- **Program not found** (run): suggests checking PATH or using an absolute program path.
- **Permission denied** (run): platform-specific hint — `chmod +x` on Unix, `Unblock-File` on Windows.

## Help overlay

`?` opens a centered overlay listing every binding. The two tables (this overlay and the persistent keybindings hint bar at the bottom of the dashboard) are maintained by hand and kept in sync with the keymap in [`crate::event::Action::from_key`](https://github.com/stescobedo/hide-env-keys/blob/master/crates/evault-tui/src/event.rs).
