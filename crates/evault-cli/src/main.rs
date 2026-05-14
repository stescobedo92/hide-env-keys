//! `evault` — secure cross-platform CLI for managing environment variables.
//!
//! By default the binary opens a persistent backend backed by SQLite
//! (encrypted with SQLCipher when the feature is enabled at build
//! time) and the OS keyring; the `--demo` / `--ephemeral` flags swap
//! in an in-memory backend for testing or kicking the tires.
#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::doc_markdown,
    clippy::needless_pass_by_value
)]

mod backend;
mod commands;
mod error;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use evault_core::model::{Group, Profile};

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
                  Subcommands operate non-interactively for scripting and CI.\n\
                  \n\
                  By default `evault` opens a persistent backend: metadata \
                  in the platform data dir, secret values in the OS keyring, \
                  and a master key bootstrapped from / into that keyring on \
                  first run. Use --demo for an ephemeral seeded backend or \
                  --ephemeral for a clean in-memory store."
)]
struct Cli {
    /// Use an ephemeral in-memory backend pre-populated with 10 demo
    /// variables.
    #[arg(long, global = true, conflicts_with = "ephemeral")]
    demo: bool,
    /// Use an ephemeral in-memory backend with no seed data.
    #[arg(long, global = true)]
    ephemeral: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Launch the interactive TUI (default action).
    Tui,
    /// List all managed variables.
    Ls,
    /// Create a new variable, prompting for the value.
    Add {
        /// Variable name (must match `[A-Z_][A-Z0-9_]*`).
        name: String,
        /// Store the value in the OS keyring rather than the metadata DB.
        #[arg(long)]
        secret: bool,
        /// Logical group: `user` (default), `system`, `project`, or a
        /// custom string.
        #[arg(long, default_value = "user")]
        group: String,
    },
    /// Delete a variable (interactive confirm unless --yes).
    Rm {
        /// Variable name.
        name: String,
        /// Skip the interactive y/N prompt. Required for scripts.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Link a variable to a project's `evault.toml` manifest.
    Link {
        /// Variable name (must already exist in the registry).
        name: String,
        /// Project root (directory). Must exist; will be canonicalised.
        #[arg(long)]
        project: PathBuf,
        /// Profile name; defaults to `default`.
        #[arg(long)]
        profile: Option<String>,
        /// Alias to expose the variable under within the project
        /// (defaults to the variable's own name).
        #[arg(long)]
        alias: Option<String>,
    },
    /// Materialize the project's effective environment to `.env`.
    Gen {
        /// Project root containing `evault.toml`.
        #[arg(long)]
        project: PathBuf,
        /// Profile to resolve. Defaults to `default`.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Run a child process with the project's env injected (no `.env`
    /// is written to disk).
    Run {
        /// Project root containing `evault.toml`.
        #[arg(long)]
        project: PathBuf,
        /// Profile to resolve. Defaults to `default`.
        #[arg(long)]
        profile: Option<String>,
        /// Command and arguments to execute. Pass after `--`.
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    /// Scan a source tree for env-var references and cross-reference
    /// with the registry (find orphans / unused).
    Scan {
        /// Path to scan recursively.
        path: PathBuf,
    },
    /// Import variables from a `.env` file (non-destructive: existing
    /// names are skipped).
    Import {
        /// File to read.
        path: PathBuf,
        /// Mark imported values as secrets (store in keyring).
        #[arg(long)]
        secret: bool,
        /// Logical group. Defaults to `user`.
        #[arg(long, default_value = "user")]
        group: String,
    },
    /// Export the registry as `.env` lines on stdout.
    Export {
        /// Replace secret values with `*****`.
        #[arg(long)]
        mask: bool,
    },
}

impl Command {
    /// Verbatim subcommand name. Reserved for future stubbed
    /// subcommands so a precise `'<name>' is not yet implemented`
    /// error can surface; nothing is stubbed at present.
    #[allow(dead_code)]
    const fn name(&self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Ls => "ls",
            Self::Add { .. } => "add",
            Self::Rm { .. } => "rm",
            Self::Link { .. } => "link",
            Self::Gen { .. } => "gen",
            Self::Run { .. } => "run",
            Self::Scan { .. } => "scan",
            Self::Import { .. } => "import",
            Self::Export { .. } => "export",
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
        Err(CliError::Io(e)) => {
            eprintln!("evault: {e}");
            ExitCode::FAILURE
        }
    }
}

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
        Command::Ls => commands::ls::run(&backend),
        Command::Add {
            name,
            secret,
            group,
        } => commands::add::run(&backend, &name, secret, parse_group(&group)),
        Command::Rm { name, yes } => commands::rm::run(&backend, &name, yes),
        Command::Link {
            name,
            project,
            profile,
            alias,
        } => commands::link::run(&backend, &name, &project, parse_profile(profile), alias),
        Command::Gen { project, profile } => {
            commands::gen::run(&backend, &project, parse_profile(profile))
        }
        Command::Run {
            project,
            profile,
            argv,
        } => {
            let (cmd, args) = argv.split_first().ok_or_else(|| {
                CliError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "evault run: no command supplied after `--`",
                ))
            })?;
            // Forward the child's exit code as the binary's exit
            // code by returning early via the `?` chain. We can't
            // return `ExitCode` from `dispatch` cleanly, so we
            // process::exit here — the terminal has not been
            // touched by the runner, so no cleanup is required.
            let exit = commands::run::run(&backend, &project, parse_profile(profile), cmd, args)?;
            std::process::exit(match exit {
                ec if ec == ExitCode::SUCCESS => 0,
                _ => 1,
            });
        }
        Command::Scan { path } => commands::scan::run(&backend, &path),
        Command::Import {
            path,
            secret,
            group,
        } => commands::import::run(&backend, &path, secret, parse_group(&group)),
        Command::Export { mask } => commands::export::run(&backend, mask),
    }
}

/// Parse a `--profile` argument (or default).
fn parse_profile(raw: Option<String>) -> Profile {
    raw.map_or_else(Profile::default_profile, Profile::named)
}

/// Parse a `--group` argument. Recognises the three canonical names
/// (`user`, `system`, `project`); any other string becomes a
/// [`Group::Custom`].
fn parse_group(raw: &str) -> Group {
    match raw {
        "user" => Group::User,
        "system" => Group::System,
        "project" => Group::Project,
        other => Group::Custom(other.to_owned()),
    }
}
