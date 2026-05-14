//! `evault reset [--yes]` — nuclear option for a corrupted or
//! incompatible persistent backend.
//!
//! Deletes the metadata DB file and the master-key keyring entry,
//! letting the next `evault` invocation bootstrap a fresh backend.
//! Loses every variable currently in the registry, so it asks for
//! confirmation unless `--yes` is passed.

use std::io::Write;
use std::path::PathBuf;

use crate::backend::sqlcipher::{KEYRING_SERVICE, MASTER_KEY_KEYRING_USER};
use crate::error::CliError;

/// Resolve the canonical DB path the way `SqlCipherBackend::open_or_init`
/// does. Duplicated locally so this command does NOT need to open the
/// backend (the whole point is that opening it fails).
fn default_db_path() -> Result<PathBuf, CliError> {
    let base = dirs::data_dir().ok_or_else(|| {
        CliError::Io(std::io::Error::other(
            "could not determine a data directory for the current platform",
        ))
    })?;
    Ok(base.join("evault").join("db.sqlite"))
}

/// Delete the metadata DB + the master-key keyring entry.
///
/// # Errors
/// Returns [`CliError::Io`] on user-cancel, on filesystem failure
/// removing the DB, or on keyring failure removing the entry.
pub fn run(yes: bool) -> Result<(), CliError> {
    let db_path = default_db_path()?;

    println!("evault reset will delete:");
    println!("  - metadata DB: {}", db_path.display());
    println!(
        "  - OS keyring master key: service `{KEYRING_SERVICE}`, user \
         `{MASTER_KEY_KEYRING_USER}`"
    );
    println!();
    println!("Every managed variable will be permanently lost.");
    println!();

    if !yes && !confirm_interactive()? {
        println!("cancelled");
        return Ok(());
    }

    // 1. DB file — non-fatal if absent.
    match std::fs::remove_file(&db_path) {
        Ok(()) => println!("removed {}", db_path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("(no DB at {})", db_path.display());
        }
        Err(e) => {
            return Err(CliError::Io(std::io::Error::other(format!(
                "remove DB '{}': {e}",
                db_path.display()
            ))));
        }
    }

    // Also try to remove any sibling -wal / -shm SQLite journal files
    // — they survive a crashed write and confuse a fresh open.
    for ext in ["-wal", "-shm", "-journal"] {
        let aux = db_path.with_file_name(format!(
            "{}{ext}",
            db_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("db.sqlite")
        ));
        if let Err(e) = std::fs::remove_file(&aux) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("warning: could not remove sibling '{}': {e}", aux.display());
            }
        }
    }

    // 2. Keyring master key entry — non-fatal if absent.
    match keyring_core::Entry::new(KEYRING_SERVICE, MASTER_KEY_KEYRING_USER) {
        Ok(entry) => match entry.delete_credential() {
            Ok(()) => println!("removed master-key keyring entry"),
            Err(keyring_core::Error::NoEntry) => {
                println!("(no master-key keyring entry)");
            }
            Err(e) => {
                eprintln!("warning: could not delete keyring entry: {e}");
            }
        },
        Err(e) => {
            eprintln!("warning: could not access keyring entry: {e}");
        }
    }

    println!();
    println!("done. Re-run `evault` to bootstrap a fresh backend.");
    Ok(())
}

fn confirm_interactive() -> Result<bool, CliError> {
    let mut stdout = std::io::stdout().lock();
    write!(stdout, "type `RESET` to confirm: ").map_err(CliError::Io)?;
    stdout.flush().map_err(CliError::Io)?;
    drop(stdout);
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).map_err(CliError::Io)?;
    Ok(buf.trim() == "RESET")
}
