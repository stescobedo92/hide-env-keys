//! `evault doctor` — local diagnostics without opening secret values.

use std::io::Write;
use std::path::PathBuf;

use evault_store_keyring::OsKeyringSecretStore;

use crate::error::CliError;
use crate::presentation;

/// Run local environment checks.
///
/// # Errors
/// Returns [`CliError`] only if writing diagnostic output fails. Backend and
/// keyring failures are reported as warning rows so `doctor` remains usable
/// when the persistent backend cannot open.
pub fn run() -> Result<(), CliError> {
    println!("{}", presentation::accent("evault doctor"));
    println!(
        "  sqlcipher: {}",
        if evault_store_sqlcipher::SQLCIPHER_ENABLED {
            presentation::success("enabled")
        } else {
            presentation::warning("disabled (SQLite bundle only)")
        }
    );

    match default_db_path() {
        Some(path) => {
            println!("  data db:   {}", path.display());
            let state = if path.exists() {
                presentation::success("present")
            } else {
                presentation::muted("not created yet")
            };
            println!("  db state:  {state}");
        }
        None => {
            println!(
                "  data db:   {}",
                presentation::warning("could not resolve platform data dir")
            );
        }
    }

    match OsKeyringSecretStore::new() {
        Ok(_) => println!("  keyring:   {}", presentation::success("available")),
        Err(e) => println!("  keyring:   {}", presentation::warning(e.to_string())),
    }

    println!(
        "  backend:   {}",
        presentation::muted("run `evault ls` for an open check")
    );
    std::io::stdout().flush()?;
    Ok(())
}

fn default_db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|base| base.join("evault").join("db.sqlite"))
}
