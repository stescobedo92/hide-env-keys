# evault-runner

[![crates.io](https://img.shields.io/crates/v/evault-runner.svg)](https://crates.io/crates/evault-runner)
[![docs.rs](https://docs.rs/evault-runner/badge.svg)](https://docs.rs/evault-runner)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)

> Child-process runner that satisfies [`evault-core`](https://crates.io/crates/evault-core)'s `ProcessRunner` trait — spawns commands with an injected environment overlay, no `.env` on disk.

`StdProcessRunner` wraps `std::process::Command` to spawn a child with the parent's stdio inherited and a caller-supplied `KEY -> VALUE` map injected as the environment overlay.

## Safety properties

- **Name validation**: every key must satisfy the evault variable-name rules (`[A-Za-z_][A-Za-z0-9_]{0,63}`) before reaching `Command`. Invalid names are rejected with `RunnerError::Invalid` carrying the key name — never any byte from the value.
- **NUL rejection**: values containing a NUL byte are rejected. NUL would terminate the OS environment block prematurely, dropping every variable after it.
- **`EVAULT_*` strip**: the runner removes every parent env var whose name starts with `EVAULT_` before applying the overlay, so internal evault config does not leak into untrusted children. Case-insensitive on Windows, case-sensitive on Unix (matching the platform's own env-var semantics).
- **No `env_clear`**: PATH and other useful inherited variables propagate; only `EVAULT_*` is stripped.

## Install

```toml
[dependencies]
evault-core = "0.1"
evault-runner = "0.1"
```

## Example: run `printenv` with two injected variables

```rust,no_run
use std::collections::BTreeMap;
use evault_core::traits::ProcessRunner;
use evault_runner::StdProcessRunner;

let mut env = BTreeMap::new();
env.insert("NODE_ENV".to_string(), "production".to_string());
env.insert("DATABASE_URL".to_string(), "postgres://localhost".to_string());

let outcome = StdProcessRunner::new()
    .run(
        "printenv",
        &["NODE_ENV".to_string(), "DATABASE_URL".to_string()],
        std::path::Path::new("."),
        &env,
    )
    .expect("spawn");

assert_eq!(outcome.exit_code, Some(0));
```

The child inherits stdin, stdout, and stderr from the parent, so interactive programs (REPLs, build watchers, etc.) work without any special handling.

## Part of the evault workspace

Used by [evault](https://github.com/stescobedo92/hide-env-keys)'s `evault run` subcommand and by the TUI's `R` (run in project) flow. See the workspace README for the broader pipeline.

## License

[MIT](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)
