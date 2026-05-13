//! Helpers to encode/decode domain types into `SQLite` rows.
//!
//! These keep the SQL-string conversions in one place so the main module
//! stays focused on transactional flow.

use rusqlite::Row;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use evault_core::error::MetadataError;
use evault_core::model::{
    AuditAction, AuditEntry, AuditId, Group, Profile, Project, ProjectId, ProjectVar, Var, VarId,
    VarKind,
};

/// Map any `rusqlite::Error` into `MetadataError::Backend` with a category
/// label that never embeds secret material or paths.
pub fn backend(category: &'static str, e: &rusqlite::Error) -> MetadataError {
    // We deliberately stringify the SQLite error code only, not the original
    // message which may carry query parameters or schema text. Concrete codes
    // ("sqlite_constraint_unique", "sqlite_busy", ...) are operational, not
    // secret.
    let code = match e {
        rusqlite::Error::SqliteFailure(sqlite_err, _) => format!("{:?}", sqlite_err.code),
        rusqlite::Error::QueryReturnedNoRows => "no_rows".to_owned(),
        rusqlite::Error::ExecuteReturnedResults => "execute_returned_rows".to_owned(),
        _ => "rusqlite".to_owned(),
    };
    MetadataError::Backend(format!("{category}: {code}"))
}

pub const fn encode_kind(kind: VarKind) -> &'static str {
    match kind {
        VarKind::Plain => "plain",
        VarKind::Secret => "secret",
    }
}

pub fn decode_kind(raw: &str) -> Result<VarKind, MetadataError> {
    match raw {
        "plain" => Ok(VarKind::Plain),
        "secret" => Ok(VarKind::Secret),
        other => Err(MetadataError::Backend(format!(
            "unknown var kind in storage: {other}"
        ))),
    }
}

pub fn encode_group(group: &Group) -> String {
    // Canonical short names for the built-ins, custom names passed through.
    match group {
        Group::User => "user".to_owned(),
        Group::System => "system".to_owned(),
        Group::Project => "project".to_owned(),
        Group::Custom(s) => format!("custom:{s}"),
    }
}

pub fn decode_group(raw: &str) -> Group {
    match raw {
        "user" => Group::User,
        "system" => Group::System,
        "project" => Group::Project,
        other if other.starts_with("custom:") => Group::Custom(other[7..].to_owned()),
        // Defensive fallback: unknown raw label is preserved as a Custom group
        // rather than panicking. A future migration can normalize it.
        other => Group::Custom(other.to_owned()),
    }
}

pub fn encode_datetime(t: OffsetDateTime) -> Result<String, MetadataError> {
    t.format(&Rfc3339)
        .map_err(|e| MetadataError::Backend(format!("encode datetime: {e}")))
}

pub fn decode_datetime(raw: &str) -> Result<OffsetDateTime, MetadataError> {
    OffsetDateTime::parse(raw, &Rfc3339)
        .map_err(|e| MetadataError::Backend(format!("decode datetime: {e}")))
}

pub fn encode_tags(tags: &[String]) -> Result<String, MetadataError> {
    serde_json::to_string(tags).map_err(|e| MetadataError::Backend(format!("encode tags: {e}")))
}

pub fn decode_tags(raw: &str) -> Result<Vec<String>, MetadataError> {
    serde_json::from_str(raw).map_err(|e| MetadataError::Backend(format!("decode tags: {e}")))
}

pub fn decode_uuid(raw: &str) -> Result<Uuid, MetadataError> {
    Uuid::parse_str(raw).map_err(|e| MetadataError::Backend(format!("decode uuid: {e}")))
}

pub fn row_to_var(row: &Row<'_>) -> Result<Var, MetadataError> {
    let id: String = row.get(0).map_err(|e| backend("row id", &e))?;
    let name: String = row.get(1).map_err(|e| backend("row name", &e))?;
    let group_raw: String = row.get(2).map_err(|e| backend("row group", &e))?;
    let kind_raw: String = row.get(3).map_err(|e| backend("row kind", &e))?;
    let tags_raw: String = row.get(4).map_err(|e| backend("row tags", &e))?;
    let length: i64 = row.get(5).map_err(|e| backend("row length", &e))?;
    let created_at: String = row.get(6).map_err(|e| backend("row created_at", &e))?;
    let updated_at: String = row.get(7).map_err(|e| backend("row updated_at", &e))?;

    let uuid = decode_uuid(&id)?;
    Var::try_from_parts(
        VarId::from_uuid(uuid),
        name,
        decode_group(&group_raw),
        decode_kind(&kind_raw)?,
        decode_tags(&tags_raw)?,
        usize::try_from(length)
            .map_err(|_| MetadataError::Backend("length out of range".into()))?,
        decode_datetime(&created_at)?,
        decode_datetime(&updated_at)?,
    )
}

pub fn row_to_project(row: &Row<'_>) -> Result<Project, MetadataError> {
    let id: String = row.get(0).map_err(|e| backend("row id", &e))?;
    let name: String = row.get(1).map_err(|e| backend("row name", &e))?;
    let path: String = row.get(2).map_err(|e| backend("row path", &e))?;
    Ok(Project::from_parts(
        ProjectId::from_uuid(decode_uuid(&id)?),
        name,
        std::path::PathBuf::from(path),
    ))
}

pub fn row_to_project_var(row: &Row<'_>) -> Result<ProjectVar, MetadataError> {
    let project_id: String = row.get(0).map_err(|e| backend("row project_id", &e))?;
    let var_id: String = row.get(1).map_err(|e| backend("row var_id", &e))?;
    let profile: String = row.get(2).map_err(|e| backend("row profile", &e))?;
    let alias: Option<String> = row.get(3).map_err(|e| backend("row alias", &e))?;
    Ok(ProjectVar {
        project_id: ProjectId::from_uuid(decode_uuid(&project_id)?),
        var_id: VarId::from_uuid(decode_uuid(&var_id)?),
        alias,
        profile: Profile::named(profile),
    })
}

pub fn encode_action(action: &AuditAction) -> String {
    match action {
        AuditAction::Custom(s) => format!("custom:{s}"),
        other => other.as_str().to_owned(),
    }
}

pub fn decode_action(raw: &str) -> AuditAction {
    match raw {
        "created" => AuditAction::Created,
        "updated" => AuditAction::Updated,
        "deleted" => AuditAction::Deleted,
        "linked" => AuditAction::Linked,
        "unlinked" => AuditAction::Unlinked,
        "materialized" => AuditAction::Materialized,
        "run" => AuditAction::Run,
        "copied" => AuditAction::Copied,
        other if other.starts_with("custom:") => AuditAction::Custom(other[7..].to_owned()),
        other => AuditAction::Custom(other.to_owned()),
    }
}

pub fn row_to_audit_entry(row: &Row<'_>) -> Result<AuditEntry, MetadataError> {
    let id: String = row.get(0).map_err(|e| backend("row id", &e))?;
    let action: String = row.get(1).map_err(|e| backend("row action", &e))?;
    let var_id: Option<String> = row.get(2).map_err(|e| backend("row var_id", &e))?;
    let project_id: Option<String> = row.get(3).map_err(|e| backend("row project_id", &e))?;
    let note: Option<String> = row.get(4).map_err(|e| backend("row note", &e))?;
    let at: String = row.get(5).map_err(|e| backend("row at", &e))?;
    Ok(AuditEntry::from_parts(
        AuditId::from_uuid(decode_uuid(&id)?),
        decode_action(&action),
        var_id
            .map(|s| decode_uuid(&s).map(VarId::from_uuid))
            .transpose()?,
        project_id
            .map(|s| decode_uuid(&s).map(ProjectId::from_uuid))
            .transpose()?,
        note,
        decode_datetime(&at)?,
    ))
}
