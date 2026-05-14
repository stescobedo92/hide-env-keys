//! `evault add NAME [--secret] [--group GROUP]` — create a new
//! variable, prompting for its value.

use evault_core::model::{Group, VarKind};
use secrecy::SecretString;

use crate::backend::BackendOps;
use crate::error::CliError;

/// Prompt the user for a value (without echo when `--secret`) and
/// create the variable in the backend.
///
/// The prompt happens on the controlling terminal, not stdin, so
/// pipes do NOT bypass the no-echo path for secrets. The `rpassword`
/// crate handles the no-echo terminal toggle cross-platform.
///
/// # Errors
/// - [`CliError::Tui`] (carrying a `ProviderError`) on backend failure.
/// - [`CliError::Io`] if reading the value fails.
pub fn run<B: BackendOps>(
    backend: &B,
    name: &str,
    secret: bool,
    group: Group,
) -> Result<(), CliError> {
    let kind = if secret {
        VarKind::Secret
    } else {
        VarKind::Plain
    };
    let value = read_value_prompt(secret).map_err(CliError::Io)?;
    if value.is_empty() {
        return Err(CliError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "value is empty (aborting create)",
        )));
    }
    let id = backend
        .create_var(name, group, kind, SecretString::new(value.into()))
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    println!("created {name} ({})", id.as_uuid());
    Ok(())
}

/// Read the value from the terminal. `secret = true` enables
/// no-echo input; otherwise the line is echoed normally so the
/// user can verify what they typed.
fn read_value_prompt(secret: bool) -> std::io::Result<String> {
    if secret {
        rpassword::prompt_password("value (will not echo): ")
    } else {
        use std::io::Write;
        let mut stdout = std::io::stdout().lock();
        write!(stdout, "value: ")?;
        stdout.flush()?;
        drop(stdout);
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        // Strip trailing newline (and \r on Windows).
        while matches!(buf.chars().last(), Some('\n' | '\r')) {
            buf.pop();
        }
        Ok(buf)
    }
}
