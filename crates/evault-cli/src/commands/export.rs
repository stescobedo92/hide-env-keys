//! `evault export [--mask]` — print every variable as `KEY=value`
//! lines on stdout (or `KEY=*****` with `--mask`).

use clap::ValueEnum;
use evault_core::model::VarKind;
use evault_tui::VarProvider;
use secrecy::ExposeSecret;

use crate::backend::BackendOps;
use crate::error::CliError;

/// Supported export output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    /// dotenv-compatible `KEY=value` lines.
    Env,
    /// JSON array of variable objects.
    Json,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env => f.write_str("env"),
            Self::Json => f.write_str("json"),
        }
    }
}

/// Print the registry to stdout in `.env` format. `mask = true`
/// replaces secret values with `*****` so the output is safe to
/// share in bug reports.
///
/// # Errors
/// Returns [`CliError`] on backend list / get failures.
pub fn run<B: BackendOps + VarProvider>(
    backend: &B,
    mask: bool,
    format: ExportFormat,
) -> Result<(), CliError> {
    let rows = backend
        .list()
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    let mut exported = Vec::with_capacity(rows.len());
    for row in rows {
        let value = if mask && matches!(row.kind, VarKind::Secret) {
            "*****".to_owned()
        } else {
            let secret = BackendOps::get_value(backend, row.id)
                .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
                .ok_or_else(|| {
                    CliError::Io(std::io::Error::other(format!(
                        "value missing for {}",
                        row.name
                    )))
                })?;
            secret.expose_secret().to_owned()
        };
        exported.push((row, value));
    }

    match format {
        ExportFormat::Env => {
            for (row, value) in exported {
                println!("{}={}", row.name, quote_env_value(&value));
            }
        }
        ExportFormat::Json => {
            let body = exported
                .into_iter()
                .map(|(row, value)| {
                    serde_json::json!({
                        "name": row.name,
                        "group": row.group.as_str(),
                        "kind": match row.kind {
                            VarKind::Secret => "secret",
                            VarKind::Plain => "plain",
                        },
                        "value": value,
                        "linked_projects": row.linked_projects,
                        "updated_at": row.updated_at.to_string(),
                    })
                })
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::to_string_pretty(&body)
                    .map_err(|e| CliError::Io(std::io::Error::other(e)))?
            );
        }
    }
    Ok(())
}

fn quote_env_value(raw: &str) -> String {
    if raw.contains(|c: char| c.is_whitespace() || c == '"' || c == '#') {
        format!("\"{}\"", raw.replace('"', "\\\""))
    } else {
        raw.to_owned()
    }
}
