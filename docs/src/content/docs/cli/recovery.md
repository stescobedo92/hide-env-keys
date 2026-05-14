---
title: Recovery (evault reset)
description: How to recover when the backend won't open — corrupted DB, key mismatch, or future-schema state.
---

`evault` keeps two pieces of state: the metadata DB on disk and the master-key entry in the OS keyring. They are tightly coupled: the DB is opened with the key from the keyring; if either drifts from the other, the open fails.

## Symptom

On launch you see:

```
evault: cannot open backend: open database '/.../db.sqlite':
incorrect key or corrupted database
```

This means one of:

- The DB file was truncated, deleted, or corrupted (rare; usually a filesystem incident).
- The master-key entry in the OS keyring was wiped or replaced by another tool.
- A binary upgrade introduced a SQLite schema version newer than what the previous binary wrote, AND the migration could not be applied.
- The `sqlcipher` feature flag changed between builds, so the new binary tries to read the file with a different encryption discipline than the one that wrote it.

## Fix

```bash
evault reset
```

This is **destructive**. It:

1. Removes the metadata DB file plus its sibling `-wal`, `-shm`, and `-journal` files (the SQLite WAL bookkeeping that lives alongside the main DB).
2. Removes the master-key entry in the OS keyring (service `evault`, user `master-key`).

After `reset`, the next launch of `evault` regenerates a fresh master key and an empty DB — exactly the first-run path.

The reset is interactive by default: you must type the literal string `RESET` to confirm. To skip the prompt in a script, pass `-y`:

```bash
evault reset -y
```

## What `reset` does NOT delete

`reset` deliberately leaves the per-variable secret values in the keyring intact. Each variable's secret value is stored under the variable's UUID as the keyring user, and the new (empty) DB after reset will not know any of those UUIDs. The keyring entries therefore become **orphaned** — present but unreferenced.

If you want a fully clean slate, open your OS credential store after `reset` and delete every entry whose service is `evault`:

- **Windows** — Credential Manager → Windows Credentials → search for `evault`.
- **macOS** — Keychain Access → search for `evault`.
- **Linux** — `secret-tool` or `seahorse` — search for the `evault` service.

This step is optional. The orphans cost a few hundred bytes each and never affect the new registry's correctness.

## Why not `reset --keyring`?

A first draft of this subcommand had a `--keyring` flag that would walk every entry under service `evault` and delete them all. We dropped it before v0.1.0 because:

1. The keyring crate's enumeration APIs are not portable: Windows Credential Manager exposes them, macOS Keychain is restricted to interactive use, and Linux Secret Service requires a search by attribute (we'd need to record the attribute set ourselves at every `put`).
2. Walking and deleting entries we don't own is a foot-gun. A bug that classifies the wrong service as `evault` could wipe credentials the user cares about.

The manual cleanup step above is a small price for not encoding that risk into the binary.

## Avoiding the need for reset

Two practices help:

- **Don't run multiple incompatible `evault` builds against the same backend.** Pin the version your scripts and CI use, or wrap `cargo install` with a re-publish guard.
- **Back up the registry before a major upgrade**: `evault export > backup.env` (with `--mask` if the destination is not trustworthy).
