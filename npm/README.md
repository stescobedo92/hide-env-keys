# evault-cli

[![npm](https://img.shields.io/npm/v/evault-cli.svg)](https://www.npmjs.com/package/evault-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)

> Secure cross-platform TUI + CLI for managing environment variables. npm wrapper around the Rust binary published at [github.com/stescobedo92/hide-env-keys](https://github.com/stescobedo92/hide-env-keys).

The package is published as `evault-cli` on npm because the shorter name `evault` was rejected by npm's anti-squat policy (too close to the existing `vault` package). The **binary** the package installs is still called `evault`.

## Install

```bash
# Global install
npm install -g evault-cli

# Project-local
npm install --save-dev evault-cli
npx evault
```

The package uses npm `optionalDependencies` for platform binaries. The wrapper
resolves the matching optional package at runtime; no network download or script
runs during `npm install`. Supported platforms:

| Platform | Architectures |
|---|---|
| macOS | x64, arm64 (Apple Silicon) |
| Linux | x64 |
| Windows | x64 |

For platforms not listed above, install from source instead:

```bash
cargo install evault-cli
```

## Usage

```bash
# Launch the interactive TUI
evault

# Or use any subcommand non-interactively
evault ls
evault add API_KEY --secret
evault link API_KEY --project ./my-app
evault run --project ./my-app -- npm start
```

See the [main repository README](https://github.com/stescobedo92/hide-env-keys/blob/master/README.md) for the full feature set, including:

- TUI dashboard with CRUD, link-to-project, view-value, run-in-project, fuzzy filter
- Native OS keyring storage for secrets (Credential Manager / Keychain / Secret Service)
- Optional SQLCipher encryption for the metadata DB
- `evault.toml` project manifest with profiles for dev / staging / prod
- Source-code scanner that finds orphan and unused variables

## Where the binary lives

After install, the native binary is at:

```
node_modules/evault-cli-darwin-x64/bin/evault
node_modules/evault-cli-darwin-arm64/bin/evault
node_modules/evault-cli-linux-x64/bin/evault
node_modules/evault-cli-windows-x64/bin/evault.exe
```

## Troubleshooting

**`optional package ... is not installed`** — reinstall with optional dependencies enabled. Avoid `--omit=optional` for this package, or install from source with `cargo install evault-cli`.

**`unsupported platform ...`** — the npm package does not currently ship a pre-built binary for that OS/CPU pair. Install from source with `cargo install evault-cli`.

**`platform package is installed but binary is missing`** — the platform package publish is incomplete. Open an issue at [github.com/stescobedo92/hide-env-keys/issues](https://github.com/stescobedo92/hide-env-keys/issues).

## License

[MIT](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)
