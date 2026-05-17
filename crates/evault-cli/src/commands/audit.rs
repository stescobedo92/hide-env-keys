//! `evault audit` — print recent audit entries newest-first.

use evault_core::model::AuditEntry;

use crate::backend::BackendOps;
use crate::error::CliError;
use crate::presentation;

/// Print recent audit entries.
///
/// # Errors
/// Returns [`CliError`] if the backend cannot read the audit log.
pub fn run<B: BackendOps>(backend: &B, limit: usize) -> Result<(), CliError> {
    let entries = backend
        .recent_audit(limit)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    if entries.is_empty() {
        println!("{}", presentation::muted("audit: no entries"));
        return Ok(());
    }
    println!(
        "{}",
        presentation::accent(
            "TIME                         ACTION        VAR                                   PROJECT"
        )
    );
    for entry in entries {
        println!("{}", row(&entry));
    }
    Ok(())
}

fn row(entry: &AuditEntry) -> String {
    let at = entry
        .at()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::from("<invalid-time>"));
    let var = entry
        .var_id()
        .map_or_else(|| String::from("-"), |id| id.to_string());
    let project = entry
        .project_id()
        .map_or_else(|| String::from("-"), |id| id.to_string());
    format!(
        "{at:<28} {action:<13} {var:<37} {project}",
        action = entry.action().as_str()
    )
}
