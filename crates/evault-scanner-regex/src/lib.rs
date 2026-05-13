//! Source-code scanner that finds environment-variable references via a
//! per-language regex bank.
//!
//! [`RegexCodeScanner`] implements
//! [`evault_core::traits::CodeScanner`] by walking a directory tree
//! with [`walkdir`], skipping common build/cache directories
//! ([`SKIPPED_DIRS`]) and filtering to source-code file extensions,
//! then applying language-specific regex patterns to every line.
//!
//! Supported languages and the calls each language is scanned for:
//!
//! | Lang | Extensions | Patterns |
//! |------|------------|----------|
//! | JavaScript / TypeScript | `js`, `mjs`, `cjs`, `ts`, `tsx`, `jsx` | `process.env.NAME`, `process.env["NAME"]`, `process.env['NAME']` |
//! | Python | `py` | `os.getenv("NAME")`, `os.environ["NAME"]`, `os.environ.get("NAME")` |
//! | Rust   | `rs` | `std::env::var("NAME")`, `env::var("NAME")`, `var_os` variants |
//! | Go     | `go` | `os.Getenv("NAME")`, `os.LookupEnv("NAME")` |
//! | Shell  | `sh`, `bash` | `${NAME}`, `$NAME` (capital identifier only — keeps false-positive rate low) |
//!
//! Files whose contents are not valid UTF-8 are silently skipped (per
//! the trait contract). I/O failures while walking the tree propagate
//! as [`ScannerError::Io`](evault_core::error::ScannerError::Io) with
//! the offending path included in the error message.
//!
//! # Limitations
//!
//! - The scanner is *line-oriented*, not language-aware: hits inside
//!   comments and string literals are reported.
//! - Lines split on `\n` and `\r\n`; legacy bare-`\r` files collapse
//!   into a single line.
//! - Single-quoted shell strings are not honored (`echo '$FOO'`
//!   produces a hit).
//!
//! See [`RegexCodeScanner`] for the full list.
//!
//! # Examples
//!
//! ```
//! use evault_core::traits::CodeScanner;
//! use evault_scanner_regex::RegexCodeScanner;
//!
//! let dir = tempfile::tempdir().unwrap();
//! std::fs::write(
//!     dir.path().join("app.js"),
//!     "const x = process.env.DATABASE_URL;\n",
//! ).unwrap();
//!
//! let hits = RegexCodeScanner::new().scan(dir.path()).unwrap();
//! assert_eq!(hits.len(), 1);
//! assert_eq!(hits[0].name, "DATABASE_URL");
//! ```
#![forbid(unsafe_code)]

mod patterns;
mod scanner;

pub use scanner::{RegexCodeScanner, SKIPPED_DIRS};
