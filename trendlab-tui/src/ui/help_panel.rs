//! Panel 6 — Help: keyboard shortcuts and documentation.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::AppState;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, _app: &AppState) {
    let mut lines: Vec<Line> = Vec::new();

    section(&mut lines, "Global Navigation");
    key(&mut lines, "1-6", "Switch to panel by number");
    key(&mut lines, "Tab / Shift+Tab", "Cycle panels forward / back");
    key(&mut lines, "q", "Quit");
    lines.push(Line::from(""));

    section(&mut lines, "Panel 1 — Data");
    key(&mut lines, "j / k", "Move cursor down / up");
    key(&mut lines, "h / l", "Collapse / expand sector");
    key(&mut lines, "Space", "Toggle ticker selection");
    key(&mut lines, "a", "Select all tickers");
    key(&mut lines, "d", "Deselect all tickers");
    key(&mut lines, "f", "Fetch data for selected tickers");
    key(&mut lines, "s", "Search: add a custom symbol");
    key(&mut lines, "Esc", "Cancel in-progress fetch");
    lines.push(Line::from(""));

    section(&mut lines, "Panel 2 — Strategy");
    key(&mut lines, "j / k", "Navigate components and parameters");
    key(&mut lines, "h / l", "Cycle component type or adjust param");
    key(&mut lines, "Enter", "Run single backtest");
    lines.push(Line::from(""));

    section(&mut lines, "Panel 3 — Sweep (YOLO)");
    key(&mut lines, "j / k", "Navigate settings");
    key(&mut lines, "h / l", "Adjust setting value");
    key(&mut lines, "Enter", "Start YOLO mode");
    key(&mut lines, "Esc", "Stop YOLO mode");
    lines.push(Line::from(""));

    section(&mut lines, "Panel 4 — Results");
    key(&mut lines, "j / k", "Scroll leaderboard");
    key(&mut lines, "t", "Toggle session / all-time");
    key(&mut lines, "p", "Cycle risk profile (Balanced → Conservative → Aggressive → TrendOptions)");
    key(&mut lines, "Enter", "Open detail drill-down + chart");
    lines.push(Line::from(""));

    section(&mut lines, "Panel 5 — Chart");
    key(&mut lines, "", "Displays equity curve from selected result");
    lines.push(Line::from(""));

    section(&mut lines, "Panel 6 — Help (this panel)");
    key(&mut lines, "e", "Open error history overlay");
    lines.push(Line::from(""));

    section(&mut lines, "Risk Profiles");
    key(&mut lines, "Balanced", "Equal weight across all metrics");
    key(&mut lines, "Conservative", "Emphasizes tail risk, drawdown, consistency");
    key(&mut lines, "Aggressive", "Emphasizes returns, Sharpe, hit rate");
    key(&mut lines, "TrendOptions", "Emphasizes hit rate, consecutive losses, OOS Sharpe");

    let para = Paragraph::new(lines);
    f.render_widget(para, area);
}

fn section<'a>(lines: &mut Vec<Line<'a>>, title: &str) {
    lines.push(Line::from(Span::styled(title.to_string(), theme::accent_bold())));
}

fn key<'a>(lines: &mut Vec<Line<'a>>, keys: &str, desc: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("  {:>20}  ", keys), theme::accent()),
        Span::styled(desc.to_string(), theme::muted()),
    ]));
}
