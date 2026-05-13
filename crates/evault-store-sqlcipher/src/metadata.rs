//! [`SqlCipherMetadataStore`] — the encrypted backend for variable and
//! project metadata and the audit log.

use std::path::Path;
use std::sync::Mutex;

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
    conn: Mutex<Connection>,
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
        #[cfg(not(feature = "sqlcipher"))]
        {
            // Without the `sqlcipher` feature the database is not encrypted
            // at rest. The key is still required by the API so that callers
            // (and the OS keyring round-trip) remain consistent across
            // builds; we store its hex hash in a meta row to give a coarse
            // "wrong key → refuse to open" check.
            verify_or_record_key(&conn, key)?;
        }
        // Enable FK enforcement for the cascading deletes.
        conn.execute_batch("PRAGMA foreign_keys = ON")
            .map_err(|e| SqlCipherOpenError::Backend(format!("enable foreign keys: {e:?}")))?;
        // Use the more robust WAL journal mode for concurrent reads.
        conn.execute_batch("PRAGMA journal_mode = WAL")
            .map_err(|e| SqlCipherOpenError::Backend(format!("set wal: {e:?}")))?;
        migrations::run(&mut conn).map_err(|e| SqlCipherOpenError::Schema(format!("{e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
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
        _ => "rusqlite".to_owned(),
    };
    SqlCipherOpenError::Io(code)
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
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|_| ())
    .map_err(|_| SqlCipherOpenError::BadKey)
}

/// Without `SQLCipher`, record (on first open) a hex digest of the master
/// key in a `meta` table and refuse to open if a subsequent call presents
/// a key whose digest does not match.
///
/// The on-disk meta row is not a secret itself — it is a one-way
/// transform of the master key with a fixed domain separator. An
/// attacker with read access to the database can still see all
/// metadata; this check only prevents *another* `MasterKey` from
/// silently reading a database it didn't create.
#[cfg(not(feature = "sqlcipher"))]
fn verify_or_record_key(conn: &Connection, key: &MasterKey) -> Result<(), SqlCipherOpenError> {
    use std::fmt::Write as _;

    use evault_core::crypto::ExposeSecret;

    conn.execute_batch("CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)")
        .map_err(|e| SqlCipherOpenError::Backend(format!("create meta table: {e:?}")))?;
    // Domain-separated digest: SHA-256 would be ideal but we avoid pulling
    // a hash crate; instead we use a length-prefixed XOR-fold of the key
    // bytes with a fixed salt. This is a coarse "are you the same key"
    // check, not a cryptographic authenticator — it lives entirely behind
    // the `not(feature = "sqlcipher")` cfg gate and is replaced by the
    // real PRAGMA key check when SQLCipher is enabled.
    let key_hex = key.to_hex_secret();
    let mut digest = [0_u8; 32];
    let bytes = key_hex.expose_secret().as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        // Truncating `i` to a byte is intentional: the salt cycles modulo
        // 256, paired with the modulo-32 indexing of `digest`.
        let salt = u8::try_from(i & 0xFF).unwrap_or(0).wrapping_mul(31);
        digest[i % digest.len()] ^= *b ^ salt;
    }
    let mut digest_hex = String::with_capacity(64);
    for b in &digest {
        // `write!` into a String is infallible per `std::fmt::Write` docs.
        let _ = write!(digest_hex, "{b:02x}");
    }

    let existing: Option<String> = conn
        .query_row("SELECT v FROM meta WHERE k = 'key_digest'", [], |row| {
            row.get(0)
        })
        .ok();
    match existing {
        Some(prev) if prev == digest_hex => Ok(()),
        Some(_) => Err(SqlCipherOpenError::BadKey),
        None => {
            conn.execute(
                "INSERT INTO meta (k, v) VALUES ('key_digest', ?1)",
                rusqlite::params![digest_hex],
            )
            .map_err(|e| SqlCipherOpenError::Backend(format!("record key digest: {e:?}")))?;
            Ok(())
        }
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
                if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                // The only UNIQUE we have is `vars.name`. The id-conflict
                // path is handled by the ON CONFLICT clause above, so a
                // ConstraintViolation here is a duplicate name.
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
        // FK ON DELETE CASCADE handles plain_values + project_vars, but only
        // if FK enforcement is active (we enabled it in `open`). Run inside a
        // transaction for atomic cascade per the trait contract.
        let tx = conn
            .transaction()
            .map_err(|e| backend("begin delete_var", &e))?;
        tx.execute(
            "DELETE FROM vars WHERE id = ?1",
            params![id.as_uuid().to_string()],
        )
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
        let tx = conn
            .transaction()
            .map_err(|e| backend("begin delete_project", &e))?;
        tx.execute(
            "DELETE FROM projects WHERE id = ?1",
            params![id.as_uuid().to_string()],
        )
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
