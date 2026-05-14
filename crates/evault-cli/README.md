# evault-cli

[![crates.io](https://img.shields.io/crates/v/evault-cli.svg)](https://crates.io/crates/evault-cli)
[![docs.rs](https://docs.rs/evault-cli/badge.svg)](https://docs.rs/evault-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)

> Cross-platform CLI binary for [evault](https://github.com/stescobedo/hide-env-keys) — secure management of environment variables with secrets in the OS keyring and metadata in a local (optionally encrypted) SQLite database.

The binary is `evault`. Run it without arguments to launch the interactive TUI; pass a subcommand to operate non-interactively for scripts and CI.

## Install

```bash
cargo install evault-cli
```

Or grab a pre-built binary from the [GitHub Releases](https://github.com/stescobedo/hide-env-keys/releases) page.

On first run, `evault` generates a fresh 256-bit master key, stores it in your OS keyring under the service `evault`, and creates a metadata database at:

- **Linux**: `~/.local/share/evault/db.sqlite`
- **macOS**: `~/Library/Application Support/evault/db.sqlite`
- **Windows**: `%APPDATA%\evault\db.sqlite`

Nothing is sent over the network. The master key never leaves your keyring.

## Subcommands

```bash
evault                                              # launch the TUI
evault ls                                           # list all managed variables
evault add API_KEY --secret                         # create (prompts for value, no echo)
evault rm API_KEY -y                                # delete (no prompt)
evault link API_KEY --project ./my-app              # write evault.toml binding
evault gen --project ./my-app                       # materialize .env (atomic, gitignored)
evault run --project ./my-app -- npm start          # spawn child with env injected
evault scan ./my-app                                # find orphan / unused variables
evault import ./.env --secret                       # bulk-import a .env file
evault export --mask                                # export the registry (mask secrets)
evault reset                                        # wipe DB + keyring (recovery)
```

Every subcommand except `reset` also accepts `--demo` (10 seeded vars, ephemeral) or `--ephemeral` (empty, no persistence) for testing without touching your real keyring.

## Profiles

Manifests support named profiles for dev / staging / prod separation:

```bash
evault link DATABASE_URL --project ./api --profile staging
evault gen --project ./api --profile staging
evault run --project ./api --profile staging -- ./serve
```

## Security highlights

- **Master key**: 256-bit, generated with `OsRng`, stored hex-encoded in the OS keyring — never on disk in plaintext.
- **Secret values**: stored in the OS keyring under service `evault`, keyed by the variable's UUID. Wrapped in `secrecy::SecretString` so buffers are zeroized on drop.
- **`.env` materialisation**: atomic write-then-rename; sibling `.gitignore` updated automatically. CRLF and NUL byte injection in values is rejected.
- **Child process injection**: every key is name-validated, NUL bytes in values are rejected, and the `EVAULT_*` prefix is stripped from the parent environment so internal config never leaks into untrusted children.

## Recovery

If you upgrade `evault` and the new binary cannot decrypt or migrate your existing DB:

```bash
evault reset    # type RESET to confirm — this is destructive
evault          # next run generates a fresh DB + master key
```

## Documentation

Full documentation, TUI keymap, and architecture overview are in the [workspace README](https://github.com/stescobedo/hide-env-keys/blob/master/README.md).

## License

[MIT](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)
