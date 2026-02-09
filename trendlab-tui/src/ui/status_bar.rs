//! Bottom status bar — last error/status message, panel hints.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{AppState, StatusLevel};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let mut spans: Vec<Span> = Vec::new();

    // Panel hints
    spans.push(Span::styled(
        " 1:Data 2:Strategy 3:Sweep 4:Results 5:Chart 6:Help",
        theme::muted(),
    ));

    // Separator
    spans.push(Span::raw(" | "));

    // Status message
    if let Some((msg, level)) = &app.status_message {
        let style = match level {
            StatusLevel::Info => theme::accent(),
            StatusLevel::Warning => theme::warning(),
            StatusLevel::Error => theme::negative(),
        };
        spans.push(Span::styled(msg.as_str(), style));
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line);
    f.render_widget(para, area);
}
