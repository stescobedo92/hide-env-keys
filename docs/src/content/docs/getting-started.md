---
title: Install & first run
description: Get evault on your machine — pre-built binary, cargo install, or npm install — and walk through the first launch.
---

`evault` ships three ways. Pick whichever fits your stack.

## Pre-built binary

Tagged releases publish per-platform binaries to [GitHub Releases](https://github.com/stescobedo92/hide-env-keys/releases). Each archive contains the `evault` binary plus `README.md` and `LICENSE`; each archive also has a sibling `.sha256` file for integrity verification.

| Platform | Archive |
|---|---|
| Linux x86_64 | `evault-<version>-x86_64-unknown-linux-gnu.tar.xz` |
| Windows x86_64 | `evault-<version>-x86_64-pc-windows-msvc.zip` |
| macOS Intel | `evault-<version>-x86_64-apple-darwin.tar.xz` |
| macOS Apple Silicon | `evault-<version>-aarch64-apple-darwin.tar.xz` |

Verify the checksum, extract, and put the binary on your `PATH`.

## From crates.io

Requires Rust 1.94 or later.

```bash
cargo install evault-cli
```

The binary name is `evault` (the crate name is `evault-cli`, but the produced bin is `evault`).

## From npm

Wraps the same pre-built binary with a thin Node downloader, so it's `npm install`–native.

```bash
npm install -g evault-cli
# or, project-local:
npm install --save-dev evault-cli
npx evault
```

The npm package is published as `evault-cli` (the short name `evault` was rejected by npm's anti-squat policy because it's too close to the existing `vault` package). The **binary** the package installs is still called `evault` — only the `npm install ...` target name changes.

The post-install step downloads the platform-specific binary from the matching GitHub Release and verifies its SHA256 checksum before placing it in `node_modules/evault-cli/bin/`.

## First run

On the first launch, `evault`:

1. Generates a fresh 256-bit master key with the OS CSPRNG.
2. Stores it hex-encoded in the OS keyring under the service `evault` and the user `master-key`.
3. Creates an empty metadata database at:
   - **Linux**: `~/.local/share/evault/db.sqlite`
   - **macOS**: `~/Library/Application Support/evault/db.sqlite`
   - **Windows**: `%APPDATA%\evault\db.sqlite`

Nothing is sent over the network. The master key never leaves your keyring; secret values are stored alongside it; the metadata DB only contains names, lengths, and timestamps — never values.

```bash
# Launch the interactive TUI against the persistent backend
evault

# Or try it without touching your real keyring / DB:
evault --demo        # ephemeral backend pre-loaded with 10 sample variables
evault --ephemeral   # empty ephemeral backend
```

## Headless environments

The keyring-backed flow requires D-Bus + Secret Service on Linux. CI sets up `gnome-keyring-daemon` for tests; for local dev on a headless server, either install `gnome-keyring-daemon` and start it with `dbus-launch`, or use `evault --ephemeral` / `evault --demo` to bypass persistence entirely.

## Next steps

- Drive the dashboard: [TUI keybindings](/hide-env-keys/tui/keybindings/)
- Script it: [CLI reference](/hide-env-keys/cli/reference/)
- Bind variables to a project: [Manifest format](/hide-env-keys/reference/manifest/)
