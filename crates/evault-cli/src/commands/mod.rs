//! Non-interactive CLI subcommands.
//!
//! Each subcommand is a small free function generic over the
//! backend's trait bounds. The static dispatch in `main::dispatch`
//! monomorphises each call for the concrete backend type (in-memory
//! or SQLCipher), so a single subcommand body runs the same code
//! against either backing store.
//!
//! All subcommand functions return [`crate::error::CliError`] so the
//! top-level `main` can chain-walk and format errors uniformly.

#![allow(clippy::print_stdout, clippy::print_stderr)]

pub mod add;
pub mod audit;
pub mod diff;
pub mod doctor;
pub mod export;
pub mod gen;
pub mod import;
pub mod link;
pub mod ls;
pub mod reset;
pub mod rm;
pub mod run;
pub mod scan;
pub mod unlink;
