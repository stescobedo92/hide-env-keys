---
title: Security model
description: How evault protects the master key, secret values, manifest writes, and child-process invocations.
---

evault's threat model assumes the attacker can read files on disk, observe environment variables of the parent shell, and inspect the user's git history. It does **not** assume protection against an attacker with root, the OS's own privileges, or runtime access to the user's logged-in session — those defeat any user-space tool.

The mitigations below are each scoped to a single property.

## Master key

- **Size**: 256 bits (32 bytes).
- **Source**: the OS CSPRNG via `rand::rngs::OsRng`.
- **Storage**: hex-encoded in the OS keyring under service `evault`, user `master-key`.
- **Never written to disk in plaintext**. The DB is opened with the key in memory; the key never leaves the process address space.
- **Lifetime**: generated on first run, persists indefinitely. Rotating the key would require re-encrypting the DB; `evault reset` is the current path for "forget everything and start over".

## Secret values

- **Storage**: each variable's secret value is in the OS keyring under service `evault`, with the variable's UUID as the user / account.
- **Wrapper**: every secret value is held in `secrecy::SecretString`, which:
  - Marks the inner `String` as non-`Copy` and non-`Debug` so it cannot accidentally appear in logs.
  - Zeroizes the heap allocation on drop, so the value does not linger in deallocated heap pages.
- **Constant-time comparisons** for the master key, via `subtle::ConstantTimeEq`. Variable-value comparisons are not constant-time today (we never compare two values for equality at runtime; the keyring is the source of truth).

## Metadata

- **What's stored**: the variable's `name`, `group`, `kind`, `length`, `linked_projects` count, `created_at`, `updated_at`. **Never the value.**
- **At rest**: SQLite database file. With the `sqlcipher` feature enabled at build time, the file is encrypted at rest with the master key as the SQLCipher key. Otherwise the metadata is stored unencrypted on disk while secret values remain in the keyring.
- **Plain-value tier**: variables created with `--secret` off live in a separate table `plain_values` inside the same DB. They are subject to the same at-rest encryption as the rest of the metadata when `sqlcipher` is enabled.

## Variable names

- **Validation**: each name must satisfy `[A-Za-z_][A-Za-z0-9_]{0,63}` (start with a letter or underscore; then letters / digits / underscores; max 64 characters).
- **Error messages report byte offset, not byte content.** If a user pastes a value into the name field by accident, the error reads `name contains an invalid character at byte offset 12` — never the offending byte. This is intentional: a single byte from input is enough to narrow entropy in logs.

## `.env` materialisation

- **Atomic**: writes go through a sibling tempfile + `fs::rename`. A crash mid-write never leaves a half-written `.env` visible.
- **Restrictive permissions**: on Unix the tempfile is created with mode `0o600` so only the owning user can read it; on Windows the file inherits the directory's ACL.
- **Durable**: the tempfile is flushed via `sync_all` before rename, so a power loss does not leave the new file empty.
- **`.gitignore`-managed**: the sibling `.gitignore` is updated automatically to exclude the materialised file. If `.gitignore` is missing it is created.
- **Injection-rejecting**: values containing CRLF (`\r\n`) or NUL (`\0`) bytes are rejected before any disk write. CRLF could let an attacker forge an extra `KEY=VALUE` line; NUL would corrupt the file body.

## Child-process injection

`evault run` and the TUI's `R` flow share the same runner:

- **Name validation**: every key is name-validated before reaching `std::process::Command`.
- **NUL rejection**: values containing a NUL byte are rejected. NUL would terminate the OS environment block prematurely, silently dropping every variable after it.
- **`EVAULT_*` strip**: the runner removes every parent env var whose name starts with `EVAULT_` before applying the overlay. This guarantees internal evault config (e.g. `EVAULT_MASTER_KEY_PATH` if that ever existed) cannot leak into untrusted children.
  - **Windows**: the strip is case-insensitive, matching Windows' own env-var name semantics. `evault_master` and `EVAULT_MASTER` are stripped equally.
  - **Unix**: the strip is case-sensitive, matching POSIX.
- **No `env_clear`**: the parent's `PATH` and other useful inherited variables propagate. Only the `EVAULT_*` prefix is stripped, plus the overlay is applied last (so the resolved values shadow any parent variable with the same name).

## Audit log

Every mutating operation appends an entry to the audit log table with:

- An action label (`create`, `update_value`, `delete`, `link`, `unlink`, …).
- The variable id (if applicable).
- The project id (if applicable).
- An optional note string.
- A UTC timestamp.

The audit log is **append-only by convention** — there is no `DELETE FROM audit_log` API in the service layer. It is intended as a forensic record, not a moderation tool. Dumping or trimming it is a manual SQLite operation.

## What evault does NOT defend against

- **A compromised OS or running session.** A process with root or with the same UID as the user can read the keyring, the DB, and the running process's memory. evault is a user-space tool; it does not pretend to outrank the OS.
- **A keylogger.** Values typed at the TUI prompt or the CLI password prompt are read with no echo, but they cross userspace I/O paths that a kernel keylogger can observe.
- **Memory dumps of the running TUI.** Secret values pass through `SecretString` and are zeroized on drop, but a live core dump or a debugger attached to the running process can read them.
- **Side-channel attacks against the keyring backend.** evault delegates to `keyring-core`, which delegates to the platform's native credential store. Side-channel resistance is a property of those backends, not of evault.
- **Network exfiltration.** evault never makes a network call. Anything claiming to read your registry over the network is not evault.
