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
mod error;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::backend::InMemoryBackend;
use crate::error::{format_chain, CliError};

/// POSIX-conventional exit code for "command-line misuse / feature
/// unavailable". Distinct from `1` (runtime failure) so wrapper
/// scripts can branch on it.
const EXIT_UNIMPLEMENTED: u8 = 2;

#[derive(Parser, Debug)]
#[command(
    name = "evault",
    version,
    about = "Secure cross-platform manager for environment variables.",
    long_about = "Run without a subcommand to launch the interactive TUI. \
                  Subcommands (ls, add, rm, link, run, gen, scan) operate \
                  non-interactively for scripting and CI.\n\
                  \n\
                  Phase 1 ships the TUI only; subcommands marked '(stub)' \
                  in --help return exit code 2 and will be wired in phase 2."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Launch the interactive TUI (default action).
    Tui,
    /// (stub) List managed variables.
    Ls,
    /// (stub) Create a new variable.
    Add {
        /// Variable name (must match `[A-Z_][A-Z0-9_]*`).
        name: String,
        /// Store the value in the OS keyring rather than the metadata DB.
        #[arg(long)]
        secret: bool,
    },
    /// (stub) Delete a variable.
    Rm {
        /// Variable name.
        name: String,
    },
}

impl Command {
    /// Verbatim subcommand name as the user typed it. Used in error
    /// messages so the user knows which subcommand was unavailable.
    const fn name(&self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Ls => "ls",
            Self::Add { .. } => "add",
            Self::Rm { .. } => "rm",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::SubcommandUnimplemented { name }) => {
            eprintln!(
                "evault: '{name}' is not yet implemented. \
                 Launch the TUI with `evault` (no arguments) to drive \
                 the registry interactively."
            );
            ExitCode::from(EXIT_UNIMPLEMENTED)
        }
        Err(CliError::Tui(e)) => {
            // Walk the source chain so the user sees the underlying
            // cause (terminal I/O kind, provider message) rather than
            // a bare "TUI error".
            eprintln!("evault: {}", format_chain(&e));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => {
            let backend = InMemoryBackend::new();
            evault_tui::run_tui(backend)?;
            Ok(())
        }
        cmd @ (Command::Ls | Command::Add { .. } | Command::Rm { .. }) => {
            Err(CliError::SubcommandUnimplemented {
                name: cmd.name().to_owned(),
            })
        }
    }
}
