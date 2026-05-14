//! `evault export [--mask]` — print every variable as `KEY=value`
//! lines on stdout (or `KEY=*****` with `--mask`).

use evault_core::model::VarKind;
use evault_tui::VarProvider;
use secrecy::ExposeSecret;

use crate::backend::BackendOps;
use crate::error::CliError;

/// Print the registry to stdout in `.env` format. `mask = true`
/// replaces secret values with `*****` so the output is safe to
/// share in bug reports.
///
/// # Errors
/// Returns [`CliError`] on backend list / get failures.
pub fn run<B: BackendOps + VarProvider>(backend: &B, mask: bool) -> Result<(), CliError> {
    let rows = backend
        .list()
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    for row in rows {
        let value = if mask && matches!(row.kind, VarKind::Secret) {
            "*****".to_owned()
        } else {
            let secret = backend
                .get_value(row.id)
                .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
                .ok_or_else(|| {
                    CliError::Io(std::io::Error::other(format!(
                        "value missing for {}",
                        row.name
                    )))
                })?;
            // Quote values containing spaces / special chars so the
            // resulting file can be sourced by a POSIX shell.
            let raw = secret.expose_secret();
            if raw.contains(|c: char| c.is_whitespace() || c == '"' || c == '#') {
                format!("\"{}\"", raw.replace('"', "\\\""))
            } else {
                raw.to_owned()
            }
        };
        println!("{}={}", row.name, value);
    }
    Ok(())
}
