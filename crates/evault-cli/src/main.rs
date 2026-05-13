//! `evault` — secure cross-platform CLI for managing environment variables.
//!
//! Stub binary. Subcommands will be wired up in subsequent phases.
#![forbid(unsafe_code)]
// A CLI legitimately writes to stdout/stderr. The workspace-wide warning is
// intended for libraries; the binary opts out explicitly.
#![allow(clippy::print_stdout, clippy::print_stderr)]

fn main() {
    println!("evault: stub. See https://github.com/stescobedo/hide-env-keys");
}
