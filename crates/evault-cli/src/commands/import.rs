//! `evault import PATH [--secret] [--group GROUP]` — parse a `.env`
//! file and create missing vars in the registry.

use std::path::Path;

use evault_core::model::{Group, VarKind};
use secrecy::SecretString;

use crate::backend::BackendOps;
use crate::error::CliError;
use crate::presentation;

/// Parse `.env` style key=value lines, creating missing entries in
/// the registry. Existing entries (by name) are skipped — the
/// import is **non-destructive**.
///
/// Lines beginning with `#` are comments; empty lines are skipped.
/// Whitespace around `=` is trimmed. Surrounding double quotes are
/// stripped.
///
/// # Errors
/// Returns [`CliError`] on read failure, malformed lines, or
/// backend creation failures.
pub fn run<B: BackendOps>(
    backend: &B,
    path: &Path,
    secret: bool,
    group: Group,
) -> Result<(), CliError> {
    let raw = std::fs::read_to_string(path).map_err(CliError::Io)?;
    let kind = if secret {
        VarKind::Secret
    } else {
        VarKind::Plain
    };
    let mut created = 0_usize;
    let mut skipped = 0_usize;
    for (i, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed.split_once('=').ok_or_else(|| {
            CliError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line {}: missing '=' in '{trimmed}'", i + 1),
            ))
        })?;
        let key = key.trim();
        let mut value = value.trim().to_string();
        // Strip surrounding double quotes if present (and balanced).
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            value = value[1..value.len() - 1].to_string();
        }

        // Skip pre-existing names.
        if backend
            .find_var_by_name(key)
            .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
            .is_some()
        {
            skipped += 1;
            continue;
        }

        backend
            .create_var(key, group.clone(), kind, SecretString::new(value.into()))
            .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
        created += 1;
    }
    println!(
        "{}",
        presentation::success(format!(
            "imported {created} variables, skipped {skipped} already present"
        ))
    );
    Ok(())
}
