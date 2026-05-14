---
title: Keybindings
description: Every key the evault TUI listens to, organized by category.
---

The TUI is keyboard-driven end to end. Release events are filtered out on every platform so each press fires exactly one action — important on Windows, where crossterm reports both `Press` and `Release` for every key.

## Navigation

| Key | Action |
|---|---|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `g` | Jump to the first row |
| `G` | Jump to the last row |
| `PgDn` | Scroll one viewport down |
| `PgUp` | Scroll one viewport up |
| `Enter` | Open the Detail view for the selected row |
| `Tab` | Cycle to the next top-level view |
| `Esc` | Cascade: error modal → toast → filter → detail → help → quit |

## CRUD

| Key | Action |
|---|---|
| `n` | New variable (modal: name + group + kind + value) |
| `e` | Edit value of the selected variable |
| `d` | Delete the selected variable (with `[y/n]` confirm modal) |

## Per-variable

| Key | Action |
|---|---|
| `l` | Link selected variable to a project (modal: path + profile + materialize toggle) |
| `v` | View the decrypted value in a centered modal |
| `s` | Toggle secret visibility (mask / show) globally |
| `y` | Copy value to clipboard *(not yet wired)* |

## Project actions

| Key | Action |
|---|---|
| `R` | Run a command in a project with the resolved env injected (see [Run in a project](/hide-env-keys/tui/run-in-project/)) |
| `p` | Switch active profile *(not yet wired)* |

`R` is uppercase by design — lowercase `r` is bound to **Refresh**, following the `g`/`G` precedent for related-but-distinct actions.

## Search & filter

| Key | Action |
|---|---|
| `Ctrl+F` | Open the fuzzy filter overlay (powered by `nucleo-matcher`) |

The filter modal opens centered; once you press `Enter`, the modal closes but the filter remains applied. The dashboard title shows `vars (matched / total)` while a filter is active. Press `Ctrl+F` again to refine, or `Esc` to clear.

## Meta

| Key | Action |
|---|---|
| `r` | Refresh from the backend |
| `?` | Toggle the help overlay |
| `q` | Quit cleanly |
| `Ctrl+C` | Quit cleanly (universal escape hatch) |

`Ctrl+C` is intercepted by the TUI so the parent shell's SIGINT handler does not fire while the terminal is in raw mode — that would leave you with a broken cursor and no echo on the prompt.

## Modal-only keys

When a modal is focused (delete confirm, editor, link form, run form, view value, error), key dispatch is intercepted. The modal handles its own keys (`Tab` to cycle fields, `Enter` to submit, `Esc` to cancel) and swallows the rest so you cannot accidentally trigger a new action while answering one.

See [Modals & error UX](/hide-env-keys/tui/modals/) for the per-modal behaviour and the cascading `Esc` semantics.
