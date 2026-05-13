//! `evault` — secure cross-platform CLI for managing environment variables.
//!
//! Phase 1: launches the TUI against an in-memory backend so the
//! interface is runnable end-to-end. Subcommands (`ls`, `add`, `rm`,
//! `link`, `run`, `gen`, `scan`) are stubbed and will be wired in
//! subsequent phases. Real persistence (`SQLCipher` + OS keyring) is
//! also a follow-up phase.
#![forbid(unsafe_code)]
// A CLI legitimately writes to stdout/stderr. The workspace-wide
// warning is intended for libraries; the binary opts out explicitly.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod backend;

use clap::{Parser, Subcommand};

use crate::backend::InMemoryBackend;

#[derive(Parser, Debug)]
#[command(
    name = "evault",
    version,
    about = "Secure cross-platform manager for environment variables.",
    long_about = "Run without a subcommand to launch the interactive TUI. \
                  Subcommands (ls, add, rm, link, run, gen, scan) operate \
                  non-interactively for scripting and CI."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Launch the interactive TUI (default action).
    Tui,
    /// List managed variables (not yet implemented in phase 1).
    Ls,
    /// Create a new variable (not yet implemented).
    Add {
        /// Variable name (must match `[A-Z_][A-Z0-9_]*`).
        name: String,
        /// Store the value in the OS keyring rather than the metadata DB.
        #[arg(long)]
        secret: bool,
    },
    /// Delete a variable (not yet implemented).
    Rm {
        /// Variable name.
        name: String,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("evault: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => {
            let backend = InMemoryBackend::new();
            evault_tui::run_tui(backend)?;
            Ok(())
        }
        Command::Ls | Command::Add { .. } | Command::Rm { .. } => {
            eprintln!(
                "evault: this subcommand is not yet implemented in phase 1. \
                 Launch the TUI with `evault` (no arguments) to drive the \
                 registry interactively."
            );
            Err("subcommand not implemented".into())
        }
    }
}
