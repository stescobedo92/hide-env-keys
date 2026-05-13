//! Detail view — per-variable inspection screen.
//!
//! Reachable from the dashboard by pressing `Enter` on a selected
//! row. Read-only in phase 2b1; phase 2b2 will wire `d` to a delete
//! confirmation modal and phase 2c will wire `e` to the editor.

use evault_core::model::VarKind;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::provider::VarSummary;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(var) = app.selected_row() else {
        // Should not happen — the dashboard refuses to open detail
        // without a selection — but render a graceful empty pane just
        // in case the row disappeared between selection and refresh.
        let block = Block::bordered().title(" detail ");
        let para = Paragraph::new("no row available")
            .block(block)
            .style(theme.dim_cell());
        frame.render_widget(para, area);
        return;
    };

    let block = Block::bordered().title(format!(" detail — {} ", var.name));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Two-column key/value layout. Left column is the label (in the
    // accent color), right column the value (default), separated by
    // a one-cell gutter.
    let lines = body_lines(var, app.secrets_visible(), theme);
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });

    // Reserve the bottom row for action hints so the user knows what
    // they can do from the detail view (especially `Esc` to return).
    let [body_area, hints_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_widget(para, body_area);

    let hints = Line::from(vec![
        Span::styled(" Esc", Style::new().fg(theme.accent)),
        Span::styled(" back to dashboard   ", theme.dim_cell()),
        Span::styled("s", Style::new().fg(theme.accent)),
        Span::styled(" toggle secret visibility   ", theme.dim_cell()),
        Span::styled("?", Style::new().fg(theme.accent)),
        Span::styled(" help", theme.dim_cell()),
    ]);
    frame.render_widget(Paragraph::new(hints), hints_area);
}

fn body_lines(var: &VarSummary, secrets_visible: bool, theme: &Theme) -> Vec<Line<'static>> {
    let label = Style::new().fg(theme.accent);
    let kind_text = match var.kind {
        VarKind::Secret => "secret",
        VarKind::Plain => "plain",
    };
    let value_len = if matches!(var.kind, VarKind::Secret) && !secrets_visible {
        "·····  (toggle with `s`)".to_owned()
    } else {
        format!("{} chars", var.value_len)
    };
    vec![
        row("Name", label, var.name.clone()),
        row("Group", label, var.group.as_str().to_owned()),
        row(
            "Kind",
            label,
            if matches!(var.kind, VarKind::Secret) {
                format!("{kind_text}  (stored in OS keyring)")
            } else {
                format!("{kind_text}  (stored in metadata DB)")
            },
        ),
        row("Value length", label, value_len),
        row("Linked projects", label, var.linked_projects.to_string()),
        row("Updated", label, format_iso8601(var.updated_at)),
        Line::raw(""),
        Line::from(Span::styled("  ID:", theme.dim_cell())),
        Line::from(Span::styled(
            format!("  {}", var.id.as_uuid()),
            theme.dim_cell(),
        )),
    ]
}

fn row(label: &str, label_style: Style, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<18}"), label_style),
        Span::raw(value),
    ])
}

/// Render an [`OffsetDateTime`] in the unambiguous `YYYY-MM-DDTHH:MM:SSZ`
/// form. Same rationale as `dashboard::format_short_date`: built by
/// hand so the formatter cannot fail and require error swallowing.
fn format_iso8601(t: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
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
    use evault_core::model::{Group, VarId};
    use time::macros::datetime;

    #[test]
    fn iso8601_format_zero_pads_and_trails_z() {
        let t = datetime!(2026-03-05 14:07:09 UTC);
        assert_eq!(format_iso8601(t), "2026-03-05T14:07:09Z");
    }

    #[test]
    fn body_lines_render_secret_kind_with_keyring_hint() {
        let theme = Theme::dark();
        let var = VarSummary {
            id: VarId::new_v4(),
            name: "API_KEY".into(),
            group: Group::User,
            kind: VarKind::Secret,
            value_len: 42,
            linked_projects: 2,
            updated_at: datetime!(2026-03-05 14:07 UTC),
        };
        let lines = body_lines(&var, false, &theme);
        let kind_line = format!("{:?}", lines.get(2).expect("kind row"));
        assert!(kind_line.contains("secret"));
        assert!(kind_line.contains("keyring"));

        // Secret length must be masked when `secrets_visible` is false.
        let len_line = format!("{:?}", lines.get(3).expect("len row"));
        assert!(len_line.contains("·····"));
    }

    #[test]
    fn body_lines_render_plain_kind_with_db_hint() {
        let theme = Theme::dark();
        let var = VarSummary {
            id: VarId::new_v4(),
            name: "NODE_ENV".into(),
            group: Group::User,
            kind: VarKind::Plain,
            value_len: 11,
            linked_projects: 0,
            updated_at: datetime!(2026-03-05 14:07 UTC),
        };
        let lines = body_lines(&var, false, &theme);
        let kind_line = format!("{:?}", lines.get(2).expect("kind row"));
        assert!(kind_line.contains("plain"));
        assert!(kind_line.contains("metadata"));
    }
}
