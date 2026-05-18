//! Dashboard table: one row per variable.

use evault_core::model::VarKind;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::provider::VarSummary;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState, theme: &Theme) {
    let [summary_area, table_area] =
        Layout::vertical([Constraint::Length(SUMMARY_HEIGHT), Constraint::Min(1)]).areas(area);
    render_summary(frame, summary_area, app, theme);

    // Column header explicitly notes the timezone so the UPDATED
    // values cannot be mistaken for local time. Display values come
    // straight from the underlying `OffsetDateTime` without
    // conversion — see `format_short_date` below.
    let header = Row::new(vec![
        Cell::from("NAME"),
        Cell::from("GROUP"),
        Cell::from("KIND"),
        Cell::from("LEN"),
        Cell::from("PROJ"),
        Cell::from("UPDATED (UTC)"),
    ])
    .style(theme.header())
    .bottom_margin(1);

    let secrets_visible = app.secrets_visible();
    let total_rows = app.rows().len();
    // Renders the rows currently *visible* (i.e. passing the active
    // fuzzy filter, or all rows when no filter is applied). The
    // dashboard never sees raw row indices — they would diverge from
    // `TableState::selected()` when a filter is on.
    let rows: Vec<Row<'static>> = app
        .visible_rows()
        .map(|v| build_row(v, secrets_visible, theme))
        .collect();
    let visible_count = rows.len();

    let widths = [
        Constraint::Min(20),    // NAME
        Constraint::Length(8),  // GROUP
        Constraint::Length(8),  // KIND
        Constraint::Length(6),  // LEN
        Constraint::Length(6),  // PROJ
        Constraint::Length(17), // UPDATED (YYYY-MM-DD HH:MM)
    ];

    let title = if visible_count == total_rows {
        format!(" variables ({total_rows}) ")
    } else {
        format!(" variables ({visible_count}/{total_rows}) ")
    };
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(theme.dim))
                .title(title)
                .title_alignment(Alignment::Center),
        )
        .column_spacing(1)
        .row_highlight_style(theme.selected_row())
        .highlight_symbol("▌ ");

    frame.render_stateful_widget(table, table_area, app.table_state_mut());
}

const SUMMARY_HEIGHT: u16 = 3;

/// Map a terminal row coordinate to the visible dashboard row underneath it.
pub fn row_index_at(area: Rect, mouse_y: u16) -> Option<usize> {
    let table_y = area.y.saturating_add(SUMMARY_HEIGHT);
    // Border top + header + bottom margin. This mirrors the table definition
    // above and keeps mouse hit-testing aligned with rendered rows.
    let first_row_y = table_y.saturating_add(3);
    if mouse_y < first_row_y {
        return None;
    }
    Some(usize::from(mouse_y - first_row_y))
}

fn render_summary(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    let total = app.rows().len();
    let visible = app.visible_row_indices().len();
    let secrets = app
        .rows()
        .iter()
        .filter(|row| matches!(row.kind, VarKind::Secret))
        .count();
    let plain = total.saturating_sub(secrets);
    let linked: usize = app.rows().iter().map(|row| row.linked_projects).sum();
    let filter_text = app
        .filter_needle()
        .filter(|needle| !needle.is_empty())
        .map_or_else(
            || "filter none".to_owned(),
            |needle| format!("filter {needle}"),
        );

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.dim))
        .title(" overview ")
        .title_alignment(Alignment::Center);
    let line = Line::from(vec![
        metric("visible", &format!("{visible}/{total}"), theme),
        sep(theme),
        metric("secret", &secrets.to_string(), theme),
        sep(theme),
        metric("plain", &plain.to_string(), theme),
        sep(theme),
        metric("links", &linked.to_string(), theme),
        sep(theme),
        Span::styled(filter_text, theme.dim_cell()),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

fn metric(label: &'static str, value: &str, theme: &Theme) -> Span<'static> {
    Span::styled(
        format!("{label} {value}"),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    )
}

fn sep(theme: &Theme) -> Span<'static> {
    Span::styled("  •  ", theme.dim_cell())
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

    #[test]
    fn row_index_hit_testing_accounts_for_summary_and_table_header() {
        let area = Rect::new(0, 1, 80, 20);
        assert_eq!(row_index_at(area, 1), None);
        assert_eq!(row_index_at(area, 6), None);
        assert_eq!(row_index_at(area, 7), Some(0));
        assert_eq!(row_index_at(area, 9), Some(2));
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
