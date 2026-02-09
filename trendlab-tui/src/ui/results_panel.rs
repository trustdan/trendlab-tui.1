//! Panel 4 — Results: leaderboard table with rankings, risk profile cycling, drill-down.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::AppState;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let r = &app.results;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "Profile: {:?} | {:?} | ",
                r.risk_profile, r.session_filter
            ),
            theme::muted(),
        ),
        Span::styled(
            format!("{} entries", r.entries.len()),
            theme::accent(),
        ),
        Span::styled("  [j/k]scroll [t]oggle [p]rofile [Enter]detail", theme::muted()),
    ]));
    lines.push(Line::from(""));

    if r.entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "No results yet. Run a backtest from Panel 2 or start YOLO from Panel 3.",
            theme::muted(),
        )));
    } else {
        // Column headers
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "{:>3} {:>14} {:>12} {:>8} {:>7} {:>7} {:>6} {:>5} {:>5}",
                    "#", "Signal", "PM", "Symbol", "Sharpe", "CAGR", "MaxDD", "WR%", "Trades"
                ),
                theme::accent_bold(),
            ),
        ]));

        // Visible rows
        let visible_height = area.height.saturating_sub(4) as usize;
        let start = r.scroll_offset;
        let end = (start + visible_height).min(r.entries.len());

        for i in start..end {
            let entry = &r.entries[i];
            let is_cursor = i == r.cursor;

            let style = if is_cursor {
                theme::accent().add_modifier(Modifier::REVERSED)
            } else {
                theme::muted()
            };

            let sharpe_style = if is_cursor {
                style
            } else {
                theme::sharpe_style(entry.sharpe)
            };

            let cagr_style = if is_cursor {
                style
            } else {
                theme::metric_color(entry.cagr)
            };

            let dd_style = if is_cursor {
                style
            } else {
                theme::negative()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{:>3} ", entry.rank), style),
                Span::styled(format!("{:>14} ", truncate(&entry.signal_type, 14)), style),
                Span::styled(format!("{:>12} ", truncate(&entry.pm_type, 12)), style),
                Span::styled(format!("{:>8} ", truncate(&entry.symbol, 8)), style),
                Span::styled(format!("{:>7.2} ", entry.sharpe), sharpe_style),
                Span::styled(format!("{:>6.1}% ", entry.cagr * 100.0), cagr_style),
                Span::styled(format!("{:>5.1}% ", entry.max_drawdown * 100.0), dd_style),
                Span::styled(format!("{:>4.0}% ", entry.win_rate * 100.0), style),
                Span::styled(format!("{:>5}", entry.trade_count), style),
            ]));
        }
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}.", &s[..max - 1])
    }
}
