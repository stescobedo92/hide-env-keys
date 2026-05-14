# evault

> Secure cross-platform TUI + CLI for managing environment variables.

`evault` keeps every project's environment variables in one place: secrets in your OS's native keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service), metadata in a local SQLite database (encrypted with SQLCipher when the feature is enabled at build time), and per-project bindings in a declarative `evault.toml` manifest. Variables can be materialised into a `.env` file or injected directly into a child process via `evault run` — never touching disk in plaintext.

## Install

From source (requires Rust 1.94+):

```bash
git clone https://github.com/stescobedo/hide-env-keys
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
| `Esc` | Cascade: toast → filter → detail → help → quit |
| `Ctrl+F` | Open fuzzy filter (`nucleo-matcher`) |
| `s` | Toggle secret visibility |
| `d` | Delete selected var (with `[y/n]` confirm modal) |
| `r` | Refresh from backend |
| `?` | Help overlay |
| `q` / `Ctrl+C` | Quit cleanly |

### Command line

```bash
# Create a variable (prompts for the value, no-echo for secrets)
evault add API_KEY --secret
evault add NODE_ENV --group user

# List variables
evault ls

# Delete (with interactive y/N; -y to skip)
evault rm API_KEY
evault rm API_KEY -y

# Link a variable to a project's manifest
evault link API_KEY --project ./my-app

# Generate a .env from the project's manifest (atomic, .gitignore'd)
evault gen --project ./my-app

# Run a command with the env injected (no .env on disk)
evault run --project ./my-app -- npm start

# Scan a source tree for env-var references
evault scan ./my-app
# Reports ORPHANS (in code, not registry), UNUSED (in registry, not code),
# REFERENCED (in both).

# Import existing .env (non-destructive: existing names are skipped)
evault import ./my-app/.env --secret

# Export to stdout (--mask redacts secret values to *****)
evault export --mask
```

Every subcommand also accepts `--demo` / `--ephemeral` if you want to try it without touching your real keyring or DB:

```bash
evault --demo ls       # list the 10 seed vars without persistence
evault --ephemeral ls  # empty list, ephemeral backend
```

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
- **Child process injection**: validates every key (must match `[A-Z_][A-Z0-9_]*`) and rejects values containing NUL before touching `std::process::Command`. The `EVAULT_*` prefix is stripped from the parent environment before applying the overlay so internal config does not leak into untrusted children.

## Development

```bash
cargo build --workspace
cargo test --workspace                                   # ~280 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

CI runs the full gate matrix on Linux, Windows, and macOS for every push and PR. See [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

### Headless Linux note

The keyring-backed flow requires DBus + Secret Service. CI sets up `gnome-keyring-daemon` for tests; for local dev on a headless server, either install `gnome-keyring-daemon` and start it with `dbus-launch`, or use `evault --ephemeral` / `evault --demo` to bypass persistence.

## License

[MIT](./LICENSE).
