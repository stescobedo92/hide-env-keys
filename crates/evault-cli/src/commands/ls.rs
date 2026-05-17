//! `evault ls` — list managed variables to stdout.

use evault_core::model::VarKind;
use evault_tui::VarProvider;

use crate::error::CliError;
use crate::presentation;

/// Print every variable in the registry, sorted by name, as a fixed-
/// width table:
///
/// ```text
/// NAME            GROUP    KIND    LEN  PROJ  UPDATED (UTC)
/// API_KEY         user     secret   16     0  2026-05-13 14:07
/// DATABASE_URL    user     secret   29     2  2026-05-13 14:07
/// ```
///
/// Secret values' lengths are still surfaced (length-only, never the
/// value itself). `--secret` does NOT need to be passed; both kinds
/// are listed.
///
/// # Errors
/// Returns [`CliError::Tui`] only because [`VarProvider`] surfaces
/// failures through the same `ProviderError` shape. The actual
/// underlying error chain (storage failure, locked keyring, …) is
/// walked by `main` before exit.
pub fn run<B: VarProvider>(backend: &B) -> Result<(), CliError> {
    let rows = backend
        .list()
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    if rows.is_empty() {
        println!(
            "{}",
            presentation::muted("no variables yet. `evault add NAME` to create one.")
        );
        return Ok(());
    }

    // Column widths: pick a sensible minimum + grow to the widest
    // name in the buffer.
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let group_w = rows
        .iter()
        .map(|r| r.group.as_str().len())
        .max()
        .unwrap_or(5)
        .max(5);

    println!(
        "{:<name_w$}  {:<group_w$}  {:<6}  {:>4}  {:>4}  UPDATED (UTC)",
        presentation::accent("NAME"),
        presentation::accent("GROUP"),
        presentation::accent("KIND"),
        presentation::accent("LEN"),
        presentation::accent("PROJ"),
    );
    for row in &rows {
        let kind = match row.kind {
            VarKind::Secret => "secret",
            VarKind::Plain => "plain",
        };
        let updated = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            row.updated_at.year(),
            u8::from(row.updated_at.month()),
            row.updated_at.day(),
            row.updated_at.hour(),
            row.updated_at.minute(),
        );
        println!(
            "{:<name_w$}  {:<group_w$}  {:<6}  {:>4}  {:>4}  {}",
            row.name,
            row.group.as_str(),
            kind,
            row.value_len,
            row.linked_projects,
            updated,
        );
    }
    Ok(())
}
