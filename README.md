# evault

> Secure cross-platform TUI for managing environment variables.

`evault` is a terminal user interface and command-line tool for managing environment variables across projects. Secrets are stored in your operating system's native secure keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service), and metadata is kept in a SQLCipher-encrypted local database. Variables can be linked to projects declaratively and either materialized into a `.env` file or injected directly into a child process via `evault run` — never touching disk in plaintext.

## Status

Early development. See [progress in the workspace](./crates/).

## Quick start

```bash
# Open the interactive TUI
evault

# Or use the CLI directly
evault add API_KEY --secret
evault link API_KEY --project ./my-app
evault run --project ./my-app -- npm start
```

## Architecture

Workspace of 10 crates, with `evault-core` providing trait-based abstractions and concrete implementations isolated in separate crates so that storage backends, scanners, and runners can be swapped without touching the business logic.

| Crate | Role |
|-------|------|
| `evault-core` | Domain model, traits, services. Pure logic. |
| `evault-store-sqlcipher` | Encrypted metadata store. |
| `evault-store-keyring` | OS keyring secret store. |
| `evault-store-memory` | In-memory backends for tests. |
| `evault-manifest` | `evault.toml` parser. |
| `evault-scanner-regex` | Source-code scanner for env-var usage. |
| `evault-materializer` | `.env` generator with `.gitignore` management. |
| `evault-runner` | Child-process exec with injected env. |
| `evault-tui` | `ratatui` interface. |
| `evault-cli` | `clap` binary. |

## License

[MIT](./LICENSE).
