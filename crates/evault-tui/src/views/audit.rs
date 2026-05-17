//! Audit-log view.

use evault_core::model::AuditEntry;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Block, Cell, Row, Table};
use ratatui::Frame;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let header = Row::new(vec![
        Cell::from("TIME (UTC)"),
        Cell::from("ACTION"),
        Cell::from("VAR"),
        Cell::from("PROJECT"),
        Cell::from("NOTE"),
    ])
    .style(theme.header())
    .bottom_margin(1);

    let rows: Vec<Row<'static>> = app
        .audit_rows()
        .iter()
        .map(|entry| build_row(entry, theme))
        .collect();
    let title = format!(" audit ({}) ", rows.len());
    let widths = [
        Constraint::Length(20),
        Constraint::Length(14),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Min(16),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::bordered().title(title))
        .column_spacing(1);
    frame.render_widget(table, area);
}

fn build_row(entry: &AuditEntry, theme: &Theme) -> Row<'static> {
    let var = entry.var_id().map_or_else(
        || "-".to_owned(),
        |id| short_id(&id.as_uuid().as_hyphenated().to_string()),
    );
    let project = entry.project_id().map_or_else(
        || "-".to_owned(),
        |id| short_id(&id.as_uuid().as_hyphenated().to_string()),
    );
    Row::new(vec![
        Cell::from(Span::styled(format_time(entry.at()), theme.dim_cell())),
        Cell::from(entry.action().as_str().to_owned()),
        Cell::from(Span::styled(var, theme.dim_cell())),
        Cell::from(Span::styled(project, theme.dim_cell())),
        Cell::from(entry.note().unwrap_or("").to_owned()),
    ])
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn format_time(t: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn formats_audit_time_without_locale() {
        assert_eq!(
            format_time(datetime!(2026-05-17 09:04:03 UTC)),
            "2026-05-17 09:04:03"
        );
    }

    #[test]
    fn short_id_keeps_first_eight_chars() {
        assert_eq!(short_id("12345678-aaaa"), "12345678");
    }
}
