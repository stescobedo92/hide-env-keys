//! Versioned schema for the `SQLCipher`-backed metadata store.
//!
//! Migrations are applied in order: when opening, the current
//! `user_version` is compared to [`SCHEMA_VERSION`] and any missing
//! statements are executed inside a single transaction.
//!
//! Adding a new schema version: bump [`SCHEMA_VERSION`], append a new
//! `&[&str]` slice to [`MIGRATIONS`], and ensure the slice contains every
//! SQL statement needed to migrate from the previous version.

use rusqlite::Connection;

use crate::encoding::backend;
use evault_core::error::MetadataError;

/// Current on-disk schema version. Increment when the internal
/// `MIGRATIONS` list grows.
pub const SCHEMA_VERSION: u32 = 1;

/// Each entry is the SQL applied to step from `index` to `index + 1`.
///
/// `MIGRATIONS[0]` therefore takes a fresh database from version 0 (empty)
/// to version 1 (the initial schema).
const MIGRATIONS: &[&[&str]] = &[&[
    // Variables: name is unique across the registry.
    r#"
    CREATE TABLE vars (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL UNIQUE,
        "group"     TEXT NOT NULL,
        kind        TEXT NOT NULL CHECK (kind IN ('plain', 'secret')),
        tags        TEXT NOT NULL DEFAULT '[]',
        length      INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL,
        updated_at  TEXT NOT NULL
    )
    "#,
    "CREATE INDEX idx_vars_name ON vars(name)",
    // Plain values are stored next to the var. Secrets live in the keyring.
    r"
    CREATE TABLE plain_values (
        var_id  TEXT PRIMARY KEY,
        value   TEXT NOT NULL,
        FOREIGN KEY (var_id) REFERENCES vars(id) ON DELETE CASCADE
    )
    ",
    r"
    CREATE TABLE projects (
        id    TEXT PRIMARY KEY,
        name  TEXT NOT NULL,
        path  TEXT NOT NULL
    )
    ",
    r"
    CREATE TABLE project_vars (
        project_id  TEXT NOT NULL,
        var_id      TEXT NOT NULL,
        profile     TEXT NOT NULL,
        alias       TEXT,
        PRIMARY KEY (project_id, var_id, profile),
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
        FOREIGN KEY (var_id)     REFERENCES vars(id)     ON DELETE CASCADE
    )
    ",
    "CREATE INDEX idx_pv_var ON project_vars(var_id)",
    r"
    CREATE TABLE audit_log (
        id          TEXT PRIMARY KEY,
        action      TEXT NOT NULL,
        var_id      TEXT,
        project_id  TEXT,
        note        TEXT,
        at          TEXT NOT NULL
    )
    ",
    "CREATE INDEX idx_audit_at ON audit_log(at)",
]];

/// Read the on-disk schema version through `PRAGMA user_version`.
fn current_version(conn: &Connection) -> Result<u32, MetadataError> {
    let v: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| backend("read user_version", &e))?;
    u32::try_from(v).map_err(|_| MetadataError::Backend("user_version out of range".into()))
}

/// Apply every pending migration. Runs inside a single transaction so a
/// failure mid-way leaves the database at the previous version.
///
/// # Errors
/// Returns [`MetadataError::Backend`] if reading or writing the schema
/// fails, or if the on-disk version is **newer** than [`SCHEMA_VERSION`]
/// (which would indicate the file was opened with a future build).
pub fn run(conn: &mut Connection) -> Result<(), MetadataError> {
    let current = current_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(MetadataError::Backend(format!(
            "database schema is version {current}; this build supports up to {SCHEMA_VERSION}"
        )));
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }

    let tx = conn
        .transaction()
        .map_err(|e| backend("begin migration", &e))?;
    for (step_idx, statements) in MIGRATIONS
        .iter()
        .enumerate()
        .skip(usize::try_from(current).unwrap_or(0))
    {
        for sql in *statements {
            tx.execute_batch(sql)
                .map_err(|e| backend(format_step(step_idx), &e))?;
        }
    }
    // Bump the user_version inside the same transaction.
    tx.pragma_update(None, "user_version", i64::from(SCHEMA_VERSION))
        .map_err(|e| backend("bump user_version", &e))?;
    tx.commit().map_err(|e| backend("commit migration", &e))?;
    Ok(())
}

const fn format_step(idx: usize) -> &'static str {
    // Static strings keep the error category leak-free.
    match idx {
        0 => "migration v0->v1",
        _ => "migration",
    }
}
