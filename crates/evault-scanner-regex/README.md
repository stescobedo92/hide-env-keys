# evault-scanner-regex

[![crates.io](https://img.shields.io/crates/v/evault-scanner-regex.svg)](https://crates.io/crates/evault-scanner-regex)
[![docs.rs](https://docs.rs/evault-scanner-regex/badge.svg)](https://docs.rs/evault-scanner-regex)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)

> Source-code scanner that finds environment-variable references via a per-language regex bank — implements [`evault-core`](https://crates.io/crates/evault-core)'s `CodeScanner` trait.

`RegexCodeScanner` walks a directory tree (skipping common build and cache directories), filters to source-code file extensions, and applies language-specific regex patterns to every line. The result is a list of `(name, path, line)` hits suitable for cross-referencing against an `evault` registry to find orphan or unused variables.

## Supported languages

| Language | Extensions | Patterns detected |
|---|---|---|
| JavaScript / TypeScript | `js`, `mjs`, `cjs`, `ts`, `tsx`, `jsx` | `process.env.NAME`, `process.env["NAME"]`, `process.env['NAME']` |
| Python | `py` | `os.getenv("NAME")`, `os.environ["NAME"]`, `os.environ.get("NAME")` |
| Rust | `rs` | `std::env::var("NAME")`, `env::var("NAME")`, `var_os` variants |
| Go | `go` | `os.Getenv("NAME")`, `os.LookupEnv("NAME")` |
| Shell | `sh`, `bash` | `${NAME}`, `$NAME` (uppercase identifiers only — keeps false-positive rate low) |

## Limitations

- **Line-oriented**, not language-aware: hits inside comments and string literals are reported.
- Lines split on `\n` and `\r\n`; legacy bare-`\r` files collapse into a single line.
- Single-quoted shell strings are not honored (`echo '$FOO'` produces a hit).

## Install

```toml
[dependencies]
evault-core = "0.1"
evault-scanner-regex = "0.1"
```

## Example: scan a directory and inspect hits

```rust
use evault_core::traits::CodeScanner;
use evault_scanner_regex::RegexCodeScanner;

let dir = tempfile::tempdir().unwrap();
std::fs::write(
    dir.path().join("app.js"),
    "const x = process.env.DATABASE_URL;\nconst y = process.env.API_KEY;\n",
).unwrap();

let hits = RegexCodeScanner::new().scan(dir.path()).unwrap();
assert_eq!(hits.len(), 2);
let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
assert!(names.contains(&"DATABASE_URL"));
assert!(names.contains(&"API_KEY"));
```

## Part of the evault workspace

Powers [evault](https://github.com/stescobedo/hide-env-keys)'s `evault scan ./path` subcommand, which classifies each name as `ORPHAN` (in code, not registry), `UNUSED` (in registry, not code), or `REFERENCED` (in both).

## License

[MIT](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)
