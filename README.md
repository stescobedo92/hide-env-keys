<h1 align="center">
  <img src="docs/public/logo.svg" alt="evault" height="72" />
</h1>

<p align="center">
  <strong>Secure cross-platform TUI + CLI for managing environment variables.</strong>
</p>

<p align="center">
  <a href="https://stescobedo92.github.io/hide-env-keys/"><strong>Documentation</strong></a>
  ·
  <a href="https://crates.io/crates/evault-cli">crates.io</a>
  ·
  <a href="https://www.npmjs.com/package/evault-cli">npm</a>
  ·
  <a href="https://github.com/stescobedo92/hide-env-keys/releases">Releases</a>
</p>

<p align="center">
  <a href="https://github.com/stescobedo92/hide-env-keys/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/stescobedo92/hide-env-keys/actions/workflows/ci.yml/badge.svg" /></a>
  <a href="https://crates.io/crates/evault-cli"><img alt="crates.io" src="https://img.shields.io/crates/v/evault-cli.svg?logo=rust" /></a>
  <a href="https://www.npmjs.com/package/evault-cli"><img alt="npm" src="https://img.shields.io/npm/v/evault-cli.svg?logo=npm" /></a>
  <a href="https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg" /></a>
</p>

---

`evault` keeps every project's environment variables in one place: secrets in your OS's native keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service), metadata in a local SQLite database (encrypted with SQLCipher when the feature is enabled at build time), and per-project bindings in a declarative `evault.toml` manifest. Variables can be materialised into a `.env` file or injected directly into a child process via `evault run` — never touching disk in plaintext.

## Install

### Pre-built binaries

Tagged releases publish per-platform binaries to [GitHub Releases](https://github.com/stescobedo92/hide-env-keys/releases). Each archive contains the `evault` binary plus `README.md` and `LICENSE`; each archive also has a sibling `.sha256` for integrity verification.

Available targets:

| Platform | Archive |
|---|---|
| Linux x86_64 | `evault-<version>-x86_64-unknown-linux-gnu.tar.xz` |
| Windows x86_64 | `evault-<version>-x86_64-pc-windows-msvc.zip` |
| macOS Intel | `evault-<version>-x86_64-apple-darwin.tar.xz` |
| macOS Apple Silicon | `evault-<version>-aarch64-apple-darwin.tar.xz` |

### From source

Requires Rust 1.94+ (workspace toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml); `rustup` will pick it up automatically).

```bash
git clone https://github.com/stescobedo92/hide-env-keys
cd hide-env-keys
cargo install --path crates/evault-cli
```

The binary is `evault`. On first run it generates a fresh 256-bit master key, stores it in your OS keyring under the service `evault` and the user `master-key`, and creates a metadata DB at:

- **Linux**: `~/.local/share/evault/db.sqlite`
- **macOS**: `~/Library/Application Support/evault/db.sqlite`
- **Windows**: `%APPDATA%\evault\db.sqlite`

Nothing is sent over the network. The master key never leaves your keyring; secret values are stored alongside it; the metadata DB only contains names, lengths, and timestamps — never values.

## Quick start

### Interactive TUI

```bash
evault              # launch the TUI against your real (persistent) registry
evault --demo       # launch against an ephemeral backend pre-loaded with 10 sample vars
evault --ephemeral  # launch against an empty ephemeral backend
```

In the TUI:

| Key | Action |
|---|---|
| `j` `k` / `↓` `↑` | Move selection |
| `g` `G` | Jump to top / bottom |
| `PgDn` `PgUp` | Page navigation |
| `Enter` | Open Detail view |
| `n` | New variable (modal: name + group + kind + value) |
| `e` | Edit value of selected var |
| `d` | Delete selected var (with `[y/n]` confirm modal) |
| `l` | Link selected var to a project (modal: path + profile + materialize toggle) |
| `v` | View decrypted value of selected var in a modal |
| `R` | **Run a command in a project with env injected** (see [below](#run-commands-inside-the-tui)) |
| `s` | Toggle secret visibility (mask / show) |
| `y` | Copy value to clipboard *(not yet wired)* |
| `Ctrl+F` | Open fuzzy filter (`nucleo-matcher`) |
| `p` | Switch active profile *(not yet wired)* |
| `Tab` | Next top-level view |
| `r` | Refresh from backend |
| `?` | Help overlay |
| `Esc` | Cascade: error modal → toast → filter → detail → help → quit |
| `q` / `Ctrl+C` | Quit cleanly |

Failed actions (invalid name, missing project, spawn errors, …) surface in a centered **error modal** with a plain-English hint explaining what went wrong and how to fix it. Press `Enter` or `Esc` to acknowledge.

#### Run commands inside the TUI

Press `R` (uppercase — lowercase `r` is Refresh) from the dashboard to open the run-in-project modal:

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

`Tab` / `Shift+Tab` cycle fields; `Enter` submits. On submit the TUI:

1. Tears the alternate screen down so the child inherits a normal terminal (with stdio passthrough — the user interacts with the child directly).
2. Loads `<path>/evault.toml`, resolves every binding for the chosen profile (secrets from the keyring, inline literals from the manifest), and injects them as the child's environment.
3. Spawns the program synchronously.
4. Re-enters raw mode + alternate screen when the child exits, refreshes the dashboard, and surfaces a toast with the exit code (`ran \`npm start\` (exit 0)`).

No `.env` ever touches disk. The `Command` field tokenises on whitespace; for quoted arguments or pipes use `evault run` from the shell instead.

### Command line

```bash
# Create a variable (prompts for the value, no-echo for secrets).
# `--group` accepts user (default), system, project, or any custom string.
evault add API_KEY --secret
evault add NODE_ENV --group user

# List variables
evault ls

# Delete (with interactive y/N; -y to skip).
evault rm API_KEY
evault rm API_KEY -y

# Link a variable to a project's manifest. Optionally under a profile,
# and/or with an alias that exposes the variable under a different key
# in the project.
evault link API_KEY --project ./my-app
evault link DATABASE_URL --project ./api --profile staging --alias DB_URL

# Generate a .env from the project's manifest (atomic write, .gitignore'd
# automatically; CRLF / NUL injection in values is rejected).
evault gen --project ./my-app
evault gen --project ./api --profile staging

# Run a command with the env injected (no .env on disk).
# Anything after `--` is the program + args; the child inherits stdio.
evault run --project ./my-app -- npm start
evault run --project ./api --profile staging -- ./serve --port 8080

# Scan a source tree for env-var references and cross-reference with
# the registry. Recognises JS/TS, Python, Rust, Go, and Shell syntax.
evault scan ./my-app
# Reports ORPHANS (in code, not registry), UNUSED (in registry, not code),
# REFERENCED (in both).

# Import existing .env (non-destructive: existing names are skipped).
evault import ./my-app/.env --secret
evault import ./my-app/.env --group project

# Export to stdout (`--mask` redacts secret values to *****).
evault export
evault export --mask

# Recover from a corrupted / incompatible backend.
# Wipes the metadata DB AND the master-key keyring entry, asks for
# confirmation (type "RESET") unless `-y`. Loses every managed variable.
evault reset
evault reset -y
```

Every subcommand except `reset` also accepts `--demo` / `--ephemeral` if you want to try it without touching your real keyring or DB:

```bash
evault --demo ls       # list the 10 seed vars without persistence
evault --ephemeral ls  # empty list, ephemeral backend
```

### Recovery: when the backend won't open

If you upgrade `evault` and the new binary cannot decrypt or migrate your existing DB, you will see:

```
evault: cannot open backend: open database '/.../db.sqlite':
incorrect key or corrupted database
```

That happens when the on-disk schema is from a future build, the master key in the keyring drifted, or the file got truncated. The recovery path is:

```bash
evault reset    # type RESET to confirm — this is destructive
evault          # first run after reset generates a fresh DB + master key
```

`reset` removes the metadata DB (plus its `-wal`, `-shm`, `-journal` siblings) and the `evault / master-key` keyring entry — but NOT the per-variable secret values in the keyring (those orphan into the keyring under their UUIDs). If you want a fully clean slate, also delete entries whose service is `evault` from your OS credential store after running `reset`.

### Profiles

Manifests support named profiles. Link or generate against a specific one:

```bash
evault link DATABASE_URL --project ./api --profile staging
evault gen --project ./api --profile staging
evault run --project ./api --profile staging -- ./serve
```

## Project manifest format

`./my-app/evault.toml`:

```toml
project_id = "550e8400-e29b-41d4-a716-446655440000"
name = "my-app"

[[bindings]]
key = "DATABASE_URL"
source = { kind = "registry", var_id = "8400-..." }
profile = "default"

[[bindings]]
key = "DATABASE_URL"
source = { kind = "registry", var_id = "<staging-db-uuid>" }
profile = "staging"

[[bindings]]
key = "NODE_ENV"
source = { kind = "inline", value = "production" }
profile = "default"
```

Bindings come in two flavours:

- **`registry`** — value lives in the central registry; only its UUID is stored in the manifest.
- **`inline`** — non-sensitive literal embedded directly. Use sparingly (`NODE_ENV=production` is fine; `DATABASE_URL` is not).

## Architecture

Workspace of 10 crates. Business logic depends only on traits (`evault-core/src/traits/`); concrete implementations are isolated so backends can be swapped without churn.

| Crate | Role |
|---|---|
| `evault-core` | Domain model, traits, services. Pure logic. |
| `evault-store-sqlcipher` | Encrypted SQLite metadata store. |
| `evault-store-keyring` | OS keyring secret store. |
| `evault-store-memory` | In-memory backends for tests. |
| `evault-manifest` | TOML manifest parser. |
| `evault-scanner-regex` | Source-code scanner (JS/TS/Python/Rust/Go/Shell). |
| `evault-materializer` | `.env` generator with atomic writes + `.gitignore` management. |
| `evault-runner` | Child-process exec with injected env. |
| `evault-tui` | `ratatui`-based interactive interface. |
| `evault-cli` | `clap`-based binary, wiring it all together. |

## Security model

- **Master key**: 256-bit, generated with `OsRng` on first run. Hex-encoded, stored in the OS keyring; never written to disk in plaintext.
- **Secret values**: stored in the OS keyring under service `evault`, keyed by the variable's UUID. Constant-time compares; values are `zeroize`d on drop via `secrecy::SecretString`.
- **Metadata**: SQLite database. With the `sqlcipher` feature enabled at build time, the entire DB is encrypted at rest with the master key (requires Strawberry Perl on Windows); otherwise the metadata (names, lengths, timestamps — never values) is unencrypted while secret values remain in the keyring.
- **`.env` materialisation**: atomic write-then-rename; sibling `.gitignore` updated automatically. CRLF / NUL byte injection in values is rejected.
- **Variable names**: validated against `[A-Za-z_][A-Za-z0-9_]{0,63}` (start with letter or underscore, then letters/digits/underscore, up to 64 chars). Error messages report the offending byte **offset** but never the byte itself, so a value pasted into the name field by mistake cannot leak into logs.
- **Child process injection**: every key is name-validated, and values are rejected if they contain a NUL byte (NUL would terminate the OS environment block prematurely). The `EVAULT_*` prefix is stripped from the parent environment before applying the overlay so internal config never leaks into untrusted children. The check is case-insensitive on Windows (where env-var names are case-insensitive) and case-sensitive on Unix.

## Development

```bash
cargo build --workspace
cargo test --workspace                                   # ~270 tests (unit + integration + doctests)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

CI runs the full gate matrix on Linux, Windows, and macOS for every push and PR. See [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

### Headless Linux note

The keyring-backed flow requires DBus + Secret Service. CI sets up `gnome-keyring-daemon` for tests; for local dev on a headless server, either install `gnome-keyring-daemon` and start it with `dbus-launch`, or use `evault --ephemeral` / `evault --demo` to bypass persistence.

## License

[MIT](./LICENSE).
