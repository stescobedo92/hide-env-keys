//! Bottom-strip input prompt — captures new-var and edit-value flows.
//!
//! Shares its real estate with the fuzzy filter strip (and renders in
//! the same layout slot when active). The user knows which is which
//! by the prompt label and the colour.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, PromptMode};
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(prompt) = app.current_prompt() else {
        return;
    };
    frame.render_widget(Clear, area);

    let label_text = match &prompt.mode {
        PromptMode::NewVar => " new var (NAME=value, will be Secret in `user`) > ".to_owned(),
        PromptMode::EditValue { name, .. } => format!(" new value for {name} > "),
    };

    // Masked rendering replaces every typed char with `*` so a
    // shoulder-surfer can't read it back. The buffer is still stored
    // verbatim and submitted as-is.
    let display: String = if prompt.mask {
        "*".repeat(prompt.buffer.chars().count())
    } else {
        prompt.buffer.clone()
    };

    let label = Span::styled(label_text, Style::new().fg(theme.warning));
    let body = Span::raw(display);
    let cursor = Span::styled("\u{258C}", Style::new().fg(theme.accent));
    let hint = Span::styled("  (Enter submit \u{00b7} Esc cancel)", theme.dim_cell());

    let line = Line::from(vec![label, body, cursor, hint]);
    frame.render_widget(Paragraph::new(line), area);
}
