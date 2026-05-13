//! Transient status message rendered above the status bar.

use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(toast) = app.current_toast() else {
        return;
    };
    let style = if matches!(toast.kind, crate::app::ToastKind::Error) {
        theme.error_toast()
    } else {
        theme.info_toast()
    };
    // Clear the cell so the dashboard does not show through long
    // toasts.
    frame.render_widget(Clear, area);
    let para = Paragraph::new(Span::styled(format!(" {} ", toast.text), style));
    frame.render_widget(para, area);
}
