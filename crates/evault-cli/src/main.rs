//! `evault` — secure cross-platform CLI for managing environment variables.
//!
//! By default the binary opens a persistent backend backed by SQLite
//! (encrypted with SQLCipher when the feature is enabled at build
//! time) and the OS keyring; the `--demo` / `--ephemeral` flags swap
//! in an in-memory backend for testing or kicking the tires.
#![forbid(unsafe_code)]
// A CLI legitimately writes to stdout/stderr. The workspace-wide
// warning is intended for libraries; the binary opts out explicitly.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::doc_markdown,
    clippy::needless_pass_by_value
)]

mod backend;
mod error;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::backend::{BackendOps, InMemoryBackend, SqlCipherBackend};
use crate::error::{format_chain, CliError};
use evault_tui::{VarMutator, VarProvider};

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
                  Subcommands (ls, add, rm, link, run, gen, scan, import, \
                  export) operate non-interactively for scripting and CI.\n\
                  \n\
                  By default `evault` opens a persistent backend: metadata \
                  in ~/.local/share/evault/db.sqlite (or platform equivalent), \
                  secret values in the OS keyring, and a master key \
                  bootstrapped from / into that keyring on first run. \
                  Use --demo for an ephemeral seeded backend or --ephemeral \
                  for a clean in-memory store."
)]
struct Cli {
    /// Use an ephemeral in-memory backend pre-populated with 10 demo
    /// variables so the TUI has something to interact with. Useful
    /// for kicking the tires without touching disk or the keyring.
    #[arg(long, global = true, conflicts_with = "ephemeral")]
    demo: bool,
    /// Use an ephemeral in-memory backend with no seed data. Useful
    /// for tests / CI that want a clean slate without persistence.
    #[arg(long, global = true)]
    ephemeral: bool,

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
            eprintln!("evault: {}", format_chain(&e));
            ExitCode::FAILURE
        }
        Err(CliError::BackendOpen(e)) => {
            eprintln!("evault: cannot open backend: {}", format_chain(&e));
            eprintln!(
                "hint: try `evault --ephemeral` (no persistence) or \
                 `evault --demo` (seeded ephemeral) to bypass the \
                 persistent backend."
            );
            ExitCode::FAILURE
        }
    }
}

/// Top-level dispatch.
///
/// Static dispatch over backend type: each branch calls a generic
/// helper that the compiler monomorphises for the concrete backend.
/// No `dyn Backend` because the generic params travel through the
/// TUI's `VarProvider + VarMutator` bounds.
fn run(cli: Cli) -> Result<(), CliError> {
    let command = cli.command.unwrap_or(Command::Tui);
    if cli.demo {
        dispatch(command, InMemoryBackend::with_demo_data())
    } else if cli.ephemeral {
        dispatch(command, InMemoryBackend::new())
    } else {
        let backend = SqlCipherBackend::open_or_init()?;
        dispatch(command, backend)
    }
}

fn dispatch<B>(command: Command, backend: B) -> Result<(), CliError>
where
    B: VarProvider + VarMutator + BackendOps,
{
    match command {
        Command::Tui => {
            evault_tui::run_tui(backend)?;
            Ok(())
        }
        Command::Ls | Command::Add { .. } | Command::Rm { .. } => {
            Err(CliError::SubcommandUnimplemented {
                name: command.name().to_owned(),
            })
        }
    }
}
