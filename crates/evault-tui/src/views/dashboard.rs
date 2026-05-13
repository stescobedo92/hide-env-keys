//! Dashboard table: one row per variable.

use evault_core::model::VarKind;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Block, Cell, Row, Table};
use ratatui::Frame;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::provider::VarSummary;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState, theme: &Theme) {
    let header = Row::new(vec![
        Cell::from("NAME"),
        Cell::from("GROUP"),
        Cell::from("KIND"),
        Cell::from("LEN"),
        Cell::from("PROJ"),
        Cell::from("UPDATED"),
    ])
    .style(theme.header())
    .bottom_margin(1);

    let secrets_visible = app.secrets_visible();
    let row_count = app.rows().len();
    let rows: Vec<Row<'static>> = app
        .rows()
        .iter()
        .map(|v| build_row(v, secrets_visible, theme))
        .collect();

    let widths = [
        Constraint::Min(20),    // NAME
        Constraint::Length(8),  // GROUP
        Constraint::Length(8),  // KIND
        Constraint::Length(6),  // LEN
        Constraint::Length(6),  // PROJ
        Constraint::Length(17), // UPDATED (YYYY-MM-DD HH:MM)
    ];

    let title = format!(" vars ({row_count}) ");
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::bordered().title(title))
        .column_spacing(1)
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("> ");

    frame.render_stateful_widget(table, area, app.table_state_mut());
}

fn build_row(v: &VarSummary, secrets_visible: bool, theme: &Theme) -> Row<'static> {
    let kind_label = match v.kind {
        VarKind::Secret => "secret",
        VarKind::Plain => "plain",
    };
    let len_text = if matches!(v.kind, VarKind::Secret) && !secrets_visible {
        "·····".to_owned()
    } else {
        v.value_len.to_string()
    };
    let updated = format_short_date(v.updated_at);
    let kind_cell = match v.kind {
        VarKind::Secret => Cell::from(Span::styled(kind_label.to_owned(), theme.secret_cell())),
        VarKind::Plain => Cell::from(kind_label.to_owned()),
    };
    Row::new(vec![
        Cell::from(v.name.clone()),
        Cell::from(v.group.as_str().to_owned()),
        kind_cell,
        Cell::from(len_text),
        Cell::from(v.linked_projects.to_string()),
        Cell::from(Span::styled(updated, theme.dim_cell())),
    ])
}

/// Render an [`OffsetDateTime`] as `YYYY-MM-DD HH:MM` without
/// depending on `time-macros` or any locale-aware formatter. Building
/// the string manually keeps the formatter infallible — there is no
/// way for `format()` to fail or surface a hidden error.
fn format_short_date(t: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::model::{Group, VarId};
    use time::macros::datetime;

    #[test]
    fn date_format_is_year_first_minutes_precise() {
        let t = datetime!(2026-03-05 14:07 UTC);
        assert_eq!(format_short_date(t), "2026-03-05 14:07");
    }

    #[test]
    fn date_format_zero_pads_single_digit_components() {
        let t = datetime!(2026-01-02 03:04 UTC);
        assert_eq!(format_short_date(t), "2026-01-02 03:04");
    }

    fn secret_row(name: &str) -> VarSummary {
        VarSummary {
            id: VarId::new_v4(),
            name: name.into(),
            group: Group::User,
            kind: VarKind::Secret,
            value_len: 42,
            linked_projects: 2,
            updated_at: datetime!(2026-03-05 14:07 UTC),
        }
    }

    #[test]
    fn secret_length_is_masked_when_secrets_hidden() {
        let theme = Theme::dark();
        let row = build_row(&secret_row("API_KEY"), false, &theme);
        // The third cell (kind) carries the secret style; the fourth
        // (LEN) carries the masked dots. We can't trivially extract
        // the cells from a `Row`, but we can re-build with both flags
        // and check they are NOT equal (would-be regression if they
        // ever produced the same widget).
        let revealed = build_row(&secret_row("API_KEY"), true, &theme);
        assert_ne!(format!("{row:?}"), format!("{revealed:?}"));
    }
}
