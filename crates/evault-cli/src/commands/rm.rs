//! `evault rm NAME [--yes]` — delete a variable, optionally with
//! interactive confirmation.

use evault_tui::VarMutator;

use crate::backend::BackendOps;
use crate::error::CliError;
use crate::presentation;

/// Delete the variable with the given name.
///
/// When `yes = false` (the default), prompts on stdin for a `y/n`
/// confirmation. When `yes = true` (passed via `--yes`), proceeds
/// without prompting — appropriate for scripts.
///
/// # Errors
/// - [`CliError::Tui`] on backend failure.
/// - [`CliError::Io`] if reading the confirmation fails.
/// - A specific "no such variable" error when the name does not
///   resolve (exit code 1).
pub fn run<B: BackendOps + VarMutator>(backend: &B, name: &str, yes: bool) -> Result<(), CliError> {
    let var = backend
        .find_var_by_name(name)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
        .ok_or_else(|| {
            CliError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no variable named '{name}'"),
            ))
        })?;

    if !yes && !confirm_interactive(name)? {
        println!("{}", presentation::warning("cancelled"));
        return Ok(());
    }

    backend
        .delete(var.id())
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    println!("{}", presentation::success(format!("deleted {name}")));
    Ok(())
}

fn confirm_interactive(name: &str) -> Result<bool, CliError> {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    write!(stdout, "delete `{name}`? [y/N] ").map_err(CliError::Io)?;
    stdout.flush().map_err(CliError::Io)?;
    drop(stdout);
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).map_err(CliError::Io)?;
    Ok(matches!(buf.trim(), "y" | "Y" | "yes"))
}
