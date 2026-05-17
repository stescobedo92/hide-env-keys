//! [`SqlCipherMetadataStore`] — the encrypted backend for variable and
//! project metadata and the audit log.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use evault_core::crypto::MasterKey;
#[cfg(feature = "sqlcipher")]
use evault_core::crypto::{ExposeSecret, SecretString};
use evault_core::error::MetadataError;
use evault_core::model::{
    AuditEntry, Profile, Project, ProjectId, ProjectVar, Var, VarFilter, VarId,
};
use evault_core::traits::{AuditSink, MetadataStore};

use crate::encoding::{
    backend, encode_action, encode_datetime, encode_group, encode_kind, encode_tags,
    row_to_audit_entry, row_to_project, row_to_project_var, row_to_var,
};
use crate::migrations;

/// Errors that can occur while opening the encrypted store.
///
/// Distinct from [`MetadataError`] because key-rejection and IO errors at
/// open time are recoverable by the caller (e.g. prompt for a different
/// passphrase, recreate the file).
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum SqlCipherOpenError {
    /// The file system rejected the database path.
    #[error("io error opening database: {0}")]
    Io(String),

    /// The supplied key did not decrypt the database (wrong key or corrupted
    /// file). Note that `SQLCipher` reports this as a generic SQL error; this
    /// variant lets the caller distinguish authentication failure from
    /// real query failures.
    #[error("incorrect key or corrupted database")]
    BadKey,

    /// A migration could not be applied or the schema is newer than this
    /// build supports.
    #[error("schema error: {0}")]
    Schema(String),

    /// Any other backend failure during open.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Encrypted backend implementing both [`MetadataStore`] and [`AuditSink`].
///
/// The underlying [`rusqlite::Connection`] is wrapped in a [`Mutex`] so the
/// store can be shared across threads behind `&self`. Every mutating method
/// runs inside a single transaction to honour the atomicity contract
/// declared on the trait.
pub struct SqlCipherMetadataStore {
    conn: Arc<Mutex<Connection>>,
}

impl Clone for SqlCipherMetadataStore {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

impl std::fmt::Debug for SqlCipherMetadataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never leak the connection state or any path/key material.
        f.debug_struct("SqlCipherMetadataStore")
            .finish_non_exhaustive()
    }
}

impl SqlCipherMetadataStore {
    /// Open the encrypted database at `path` with `key`. Runs migrations to
    /// reach [`crate::SCHEMA_VERSION`].
    ///
    /// # Errors
    /// - [`SqlCipherOpenError::Io`] if the file cannot be opened.
    /// - [`SqlCipherOpenError::BadKey`] if the key does not decrypt the file.
    /// - [`SqlCipherOpenError::Schema`] for migration failures or future
    ///   schema versions.
    pub fn open(path: &Path, key: &MasterKey) -> Result<Self, SqlCipherOpenError> {
        let mut conn = Connection::open(path).map_err(|e| open_io(&e))?;
        #[cfg(feature = "sqlcipher")]
        {
            apply_key(&conn, &key.to_hex_secret())?;
            verify_key(&conn)?;
        }
        // Enable FK enforcement and WAL before any write inside the open
        // transaction. Both pragmas are connection-scoped and idempotent.
        pragma_setup(&conn)?;
        #[cfg(not(feature = "sqlcipher"))]
        {
            // Without the `sqlcipher` feature, the key digest check and the
            // migrations live inside a single transaction so a half-finished
            // open never leaves a "phantom" database where the meta table
            // exists but no schema does.
            init_or_verify_unencrypted(&mut conn, key)?;
        }
        #[cfg(feature = "sqlcipher")]
        {
            migrations::run(&mut conn).map_err(|e| SqlCipherOpenError::Schema(format!("{e}")))?;
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, MetadataError> {
        self.conn
            .lock()
            .map_err(|_| MetadataError::Backend("sqlcipher connection lock poisoned".into()))
    }
}

fn open_io(e: &rusqlite::Error) -> SqlCipherOpenError {
    // Carry the SQLite error code only; never a path or query string.
    let code = match e {
        rusqlite::Error::SqliteFailure(sqlite, _) => format!("{:?}", sqlite.code),
        rusqlite::Error::InvalidPath(_) => "invalid_path".to_owned(),
        _ => "rusqlite".to_owned(),
    };
    SqlCipherOpenError::Io(code)
}

fn pragma_setup(conn: &Connection) -> Result<(), SqlCipherOpenError> {
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(|e| match &e {
            rusqlite::Error::SqliteFailure(s, _) => {
                SqlCipherOpenError::Backend(format!("enable foreign keys: {:?}", s.code))
            }
            _ => SqlCipherOpenError::Backend("enable foreign keys".into()),
        })?;
    conn.execute_batch("PRAGMA journal_mode = WAL")
        .map_err(|e| match &e {
            rusqlite::Error::SqliteFailure(s, _) => {
                SqlCipherOpenError::Backend(format!("set wal: {:?}", s.code))
            }
            _ => SqlCipherOpenError::Backend("set wal".into()),
        })?;
    Ok(())
}

#[cfg(feature = "sqlcipher")]
fn apply_key(conn: &Connection, hex_key: &SecretString) -> Result<(), SqlCipherOpenError> {
    // SQLCipher accepts `x'<hex>'` raw keys: this avoids any PBKDF derivation
    // and lets us feed our own 256-bit key directly. The hex is generated by
    // `MasterKey::to_hex_secret` and contains only `[0-9a-f]`, so a simple
    // format-string concatenation is safe from quote injection.
    let pragma = format!("PRAGMA key = \"x'{}'\";", hex_key.expose_secret());
    conn.execute_batch(&pragma).map_err(|e| match e {
        rusqlite::Error::SqliteFailure(s, _) => {
            SqlCipherOpenError::Backend(format!("apply key: {:?}", s.code))
        }
        _ => SqlCipherOpenError::Backend("apply key".into()),
    })?;
    Ok(())
}

#[cfg(feature = "sqlcipher")]
fn verify_key(conn: &Connection) -> Result<(), SqlCipherOpenError> {
    // Any read that touches the encrypted header confirms the key works.
    // `sqlite_master` is the canonical SQLCipher quick-check.
    //
    // We map only `SQLITE_NOTADB` (the canonical "page contents don't look
    // like a SQLite database after decryption" signal) to `BadKey`. Other
    // errors (`SQLITE_CORRUPT`, `SQLITE_IOERR`, `SQLITE_BUSY`) propagate as
    // `Backend` so the user can distinguish a real key mismatch from a
    // failing disk or a locked file.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|_| ())
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(s, _) if s.extended_code == rusqlite::ffi::SQLITE_NOTADB => {
            SqlCipherOpenError::BadKey
        }
        rusqlite::Error::SqliteFailure(s, _) => {
            SqlCipherOpenError::Backend(format!("verify key: {:?}", s.code))
        }
        _ => SqlCipherOpenError::Backend("verify key".into()),
    })
}

/// Without `SQLCipher`, record (on first open) a SHA-256 digest of the
/// master key in a `meta` table and refuse to open if a subsequent call
/// presents a key whose digest does not match.
///
/// **Threat model**: in this mode the database file is **not** encrypted
/// at rest — an attacker with read access to the file sees every variable
/// name, project path, and audit row in plaintext. The digest check is a
/// defense against accidentally opening a database with the wrong key, not
/// a cryptographic authenticator. Use the `sqlcipher` feature for at-rest
/// encryption.
///
/// SHA-256 is domain-separated with a fixed prefix so the on-disk digest
/// cannot be confused with any other use of the master key.
///
/// The verify-or-record step runs inside a single transaction with the
/// migrations so a half-finished open never leaves a "phantom" database
/// where the meta table exists but no schema does.
#[cfg(not(feature = "sqlcipher"))]
fn init_or_verify_unencrypted(
    conn: &mut Connection,
    key: &MasterKey,
) -> Result<(), SqlCipherOpenError> {
    use std::fmt::Write as _;

    use evault_core::crypto::ExposeSecret;
    use rusqlite::OptionalExtension;
    use sha2::{Digest, Sha256};

    // Compute the digest outside the transaction; it depends only on `key`.
    let key_hex = key.to_hex_secret();
    let mut hasher = Sha256::new();
    hasher.update(b"evault-key-digest-v1\0");
    hasher.update(key_hex.expose_secret().as_bytes());
    let digest = hasher.finalize();
    let mut digest_hex = String::with_capacity(64);
    for b in digest {
        // `write!` to a `String` cannot fail (per `std::fmt::Write` impl
        // for `String`). We allow `expect_used` locally to encode the
        // invariant structurally.
        #[allow(clippy::expect_used)]
        {
            write!(digest_hex, "{b:02x}").expect("writing to String is infallible");
        }
    }

    let tx = conn
        .transaction()
        .map_err(|e| SqlCipherOpenError::Backend(format!("begin open tx: {}", error_code(&e))))?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)")
        .map_err(|e| SqlCipherOpenError::Backend(format!("create meta: {}", error_code(&e))))?;

    let existing: Option<String> = tx
        .query_row("SELECT v FROM meta WHERE k = 'key_digest'", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| SqlCipherOpenError::Backend(format!("read digest: {}", error_code(&e))))?;

    match existing {
        Some(prev) if prev == digest_hex => {}
        Some(_) => return Err(SqlCipherOpenError::BadKey),
        None => {
            tx.execute(
                "INSERT INTO meta (k, v) VALUES ('key_digest', ?1)",
                rusqlite::params![digest_hex],
            )
            .map_err(|e| {
                SqlCipherOpenError::Backend(format!("write digest: {}", error_code(&e)))
            })?;
        }
    }

    // Run migrations under the same transaction so any failure leaves the
    // file at its previous state — no phantom meta row.
    migrations::run_in_tx(&tx).map_err(|e| SqlCipherOpenError::Schema(format!("{e}")))?;

    tx.commit()
        .map_err(|e| SqlCipherOpenError::Backend(format!("commit open tx: {}", error_code(&e))))?;
    Ok(())
}

#[cfg(not(feature = "sqlcipher"))]
fn error_code(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(s, _) => format!("{:?}", s.code),
        _ => "rusqlite".to_owned(),
    }
}

impl MetadataStore for SqlCipherMetadataStore {
    fn upsert_var(&self, var: &Var) -> Result<(), MetadataError> {
        let conn = self.lock()?;
        let id = var.id().as_uuid().to_string();
        let group = encode_group(var.group());
        let kind = encode_kind(var.kind());
        let tags = encode_tags(var.tags())?;
        let created = encode_datetime(var.created_at())?;
        let updated = encode_datetime(var.updated_at())?;
        let length = i64::try_from(var.length())
            .map_err(|_| MetadataError::Backend("length out of i64 range".into()))?;
        let result = conn.execute(
            r#"
            INSERT INTO vars (id, name, "group", kind, tags, length, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                "group" = excluded."group",
                kind = excluded.kind,
                tags = excluded.tags,
                length = excluded.length,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
            params![id, var.name(), group, kind, tags, length, created, updated],
        );
        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(sqlite_err, _))
                if sqlite_err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                // Narrow specifically to the UNIQUE constraint (extended
                // code 2067). Other ConstraintViolation extended codes
                // (CHECK, NOT NULL, future UNIQUEs) propagate as Backend
                // so they cannot be silently mis-mapped to DuplicateName.
                Err(MetadataError::DuplicateName(var.name().to_owned()))
            }
            Err(e) => Err(backend("upsert_var", &e)),
        }
    }

    fn get_var(&self, id: VarId) -> Result<Option<Var>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, \"group\", kind, tags, length, created_at, updated_at \
                 FROM vars WHERE id = ?1",
            )
            .map_err(|e| backend("prepare get_var", &e))?;
        let mut rows = stmt
            .query(params![id.as_uuid().to_string()])
            .map_err(|e| backend("query get_var", &e))?;
        match rows.next().map_err(|e| backend("row get_var", &e))? {
            Some(row) => Ok(Some(row_to_var(row)?)),
            None => Ok(None),
        }
    }

    fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, \"group\", kind, tags, length, created_at, updated_at \
                 FROM vars WHERE name = ?1",
            )
            .map_err(|e| backend("prepare find_var_by_name", &e))?;
        let mut rows = stmt
            .query(params![name])
            .map_err(|e| backend("query find_var_by_name", &e))?;
        match rows
            .next()
            .map_err(|e| backend("row find_var_by_name", &e))?
        {
            Some(row) => Ok(Some(row_to_var(row)?)),
            None => Ok(None),
        }
    }

    fn list_vars(&self, filter: &VarFilter) -> Result<Vec<Var>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, \"group\", kind, tags, length, created_at, updated_at \
                 FROM vars ORDER BY name ASC",
            )
            .map_err(|e| backend("prepare list_vars", &e))?;
        let mut rows = stmt.query([]).map_err(|e| backend("query list_vars", &e))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| backend("row list_vars", &e))? {
            let var = row_to_var(row)?;
            if filter.matches(&var) {
                out.push(var);
            }
        }
        Ok(out)
    }

    fn delete_var(&self, id: VarId) -> Result<(), MetadataError> {
        let mut conn = self.lock()?;
        // Explicit cascade: three DELETEs in one transaction. We do not rely
        // solely on FK ON DELETE CASCADE because if `PRAGMA foreign_keys` is
        // ever off, the cascade silently no-ops. Explicit deletes also keep
        // the contract self-documenting at the call site.
        let tx = conn
            .transaction()
            .map_err(|e| backend("begin delete_var", &e))?;
        let id_str = id.as_uuid().to_string();
        tx.execute(
            "DELETE FROM plain_values WHERE var_id = ?1",
            params![id_str],
        )
        .map_err(|e| backend("delete_var plain_values", &e))?;
        tx.execute(
            "DELETE FROM project_vars WHERE var_id = ?1",
            params![id_str],
        )
        .map_err(|e| backend("delete_var project_vars", &e))?;
        tx.execute("DELETE FROM vars WHERE id = ?1", params![id_str])
            .map_err(|e| backend("delete_var", &e))?;
        tx.commit().map_err(|e| backend("commit delete_var", &e))?;
        Ok(())
    }

    fn get_plain_value(&self, id: VarId) -> Result<Option<String>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached("SELECT value FROM plain_values WHERE var_id = ?1")
            .map_err(|e| backend("prepare get_plain_value", &e))?;
        stmt.query_row(params![id.as_uuid().to_string()], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|e| backend("query get_plain_value", &e))
    }

    fn set_plain_value(&self, id: VarId, value: &str) -> Result<(), MetadataError> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| backend("begin set_plain_value", &e))?;

        let kind: Option<String> = tx
            .query_row(
                "SELECT kind FROM vars WHERE id = ?1",
                params![id.as_uuid().to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| backend("read kind", &e))?;
        let Some(kind_raw) = kind else {
            return Err(MetadataError::VarNotFound(id));
        };
        let actual = crate::encoding::decode_kind(&kind_raw)?;
        if actual != evault_core::model::VarKind::Plain {
            return Err(MetadataError::KindMismatch {
                id,
                expected: evault_core::model::VarKind::Plain,
                actual,
            });
        }
        tx.execute(
            r"
            INSERT INTO plain_values (var_id, value)
            VALUES (?1, ?2)
            ON CONFLICT(var_id) DO UPDATE SET value = excluded.value
            ",
            params![id.as_uuid().to_string(), value],
        )
        .map_err(|e| backend("set_plain_value", &e))?;
        tx.commit()
            .map_err(|e| backend("commit set_plain_value", &e))?;
        Ok(())
    }

    fn upsert_project(&self, project: &Project) -> Result<(), MetadataError> {
        let conn = self.lock()?;
        let path = project
            .path()
            .to_str()
            .ok_or_else(|| MetadataError::Invalid("project path is not valid UTF-8".into()))?;
        conn.execute(
            r"
            INSERT INTO projects (id, name, path)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET name = excluded.name, path = excluded.path
            ",
            params![project.id().as_uuid().to_string(), project.name(), path],
        )
        .map_err(|e| backend("upsert_project", &e))?;
        Ok(())
    }

    fn get_project(&self, id: ProjectId) -> Result<Option<Project>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached("SELECT id, name, path FROM projects WHERE id = ?1")
            .map_err(|e| backend("prepare get_project", &e))?;
        let mut rows = stmt
            .query(params![id.as_uuid().to_string()])
            .map_err(|e| backend("query get_project", &e))?;
        match rows.next().map_err(|e| backend("row get_project", &e))? {
            Some(row) => Ok(Some(row_to_project(row)?)),
            None => Ok(None),
        }
    }

    fn list_projects(&self) -> Result<Vec<Project>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached("SELECT id, name, path FROM projects ORDER BY name ASC")
            .map_err(|e| backend("prepare list_projects", &e))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| backend("query list_projects", &e))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| backend("row list_projects", &e))? {
            out.push(row_to_project(row)?);
        }
        Ok(out)
    }

    fn delete_project(&self, id: ProjectId) -> Result<(), MetadataError> {
        let mut conn = self.lock()?;
        // Explicit cascade — see `delete_var` for the rationale.
        let tx = conn
            .transaction()
            .map_err(|e| backend("begin delete_project", &e))?;
        let id_str = id.as_uuid().to_string();
        tx.execute(
            "DELETE FROM project_vars WHERE project_id = ?1",
            params![id_str],
        )
        .map_err(|e| backend("delete_project project_vars", &e))?;
        tx.execute("DELETE FROM projects WHERE id = ?1", params![id_str])
            .map_err(|e| backend("delete_project", &e))?;
        tx.commit()
            .map_err(|e| backend("commit delete_project", &e))?;
        Ok(())
    }

    fn upsert_link(&self, link: &ProjectVar) -> Result<(), MetadataError> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| backend("begin upsert_link", &e))?;
        // Existence checks first so we can return precise typed errors.
        let project_exists: bool = tx
            .query_row(
                "SELECT 1 FROM projects WHERE id = ?1",
                params![link.project_id.as_uuid().to_string()],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| backend("check project", &e))?
            .is_some();
        if !project_exists {
            return Err(MetadataError::ProjectNotFound(link.project_id));
        }
        let var_exists: bool = tx
            .query_row(
                "SELECT 1 FROM vars WHERE id = ?1",
                params![link.var_id.as_uuid().to_string()],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| backend("check var", &e))?
            .is_some();
        if !var_exists {
            return Err(MetadataError::VarNotFound(link.var_id));
        }
        tx.execute(
            r"
            INSERT INTO project_vars (project_id, var_id, profile, alias)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(project_id, var_id, profile) DO UPDATE SET alias = excluded.alias
            ",
            params![
                link.project_id.as_uuid().to_string(),
                link.var_id.as_uuid().to_string(),
                link.profile.as_str(),
                link.alias,
            ],
        )
        .map_err(|e| backend("upsert_link", &e))?;
        tx.commit().map_err(|e| backend("commit upsert_link", &e))?;
        Ok(())
    }

    fn delete_link(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: &Profile,
    ) -> Result<bool, MetadataError> {
        let conn = self.lock()?;
        let affected = conn
            .execute(
                "DELETE FROM project_vars \
                 WHERE project_id = ?1 AND var_id = ?2 AND profile = ?3",
                params![
                    project_id.as_uuid().to_string(),
                    var_id.as_uuid().to_string(),
                    profile.as_str(),
                ],
            )
            .map_err(|e| backend("delete_link", &e))?;
        Ok(affected > 0)
    }

    fn list_links_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectVar>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT project_id, var_id, profile, alias FROM project_vars \
                 WHERE project_id = ?1",
            )
            .map_err(|e| backend("prepare list_links_for_project", &e))?;
        let mut rows = stmt
            .query(params![project_id.as_uuid().to_string()])
            .map_err(|e| backend("query list_links_for_project", &e))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| backend("row link", &e))? {
            out.push(row_to_project_var(row)?);
        }
        Ok(out)
    }

    fn list_links_for_var(&self, var_id: VarId) -> Result<Vec<ProjectVar>, MetadataError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT project_id, var_id, profile, alias FROM project_vars WHERE var_id = ?1",
            )
            .map_err(|e| backend("prepare list_links_for_var", &e))?;
        let mut rows = stmt
            .query(params![var_id.as_uuid().to_string()])
            .map_err(|e| backend("query list_links_for_var", &e))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| backend("row link", &e))? {
            out.push(row_to_project_var(row)?);
        }
        Ok(out)
    }
}

impl AuditSink for SqlCipherMetadataStore {
    fn append(&self, entry: &AuditEntry) -> Result<(), MetadataError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO audit_log (id, action, var_id, project_id, note, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.id().as_uuid().to_string(),
                encode_action(entry.action()),
                entry.var_id().map(|v| v.as_uuid().to_string()),
                entry.project_id().map(|p| p.as_uuid().to_string()),
                entry.note(),
                encode_datetime(entry.at())?,
            ],
        )
        .map_err(|e| backend("audit append", &e))?;
        Ok(())
    }

    fn list(&self, limit: usize) -> Result<Vec<AuditEntry>, MetadataError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, action, var_id, project_id, note, at \
                 FROM audit_log ORDER BY at DESC, id DESC LIMIT ?1",
            )
            .map_err(|e| backend("prepare audit list", &e))?;
        let lim = i64::try_from(limit)
            .map_err(|_| MetadataError::Backend("limit out of i64 range".into()))?;
        let mut rows = stmt
            .query(params![lim])
            .map_err(|e| backend("query audit list", &e))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| backend("row audit", &e))? {
            out.push(row_to_audit_entry(row)?);
        }
        Ok(out)
    }
}
