//! Overlay widgets — welcome, detail drill-down, error history, search.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::AppState;
use crate::theme;
use crate::ui::centered_rect;

/// First-run welcome overlay.
pub fn render_welcome(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 40, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(" Welcome to TrendLab v3 ")
        .title_style(theme::accent_bold());

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Getting started:",
            theme::accent_bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  1. Press 1 to go to the Data panel",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  2. Select tickers with Space",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  3. Press f to fetch market data",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  4. Press 3 for YOLO mode once you have data",
            theme::muted(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to dismiss...",
            theme::neutral(),
        )),
    ];

    let para = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(para, popup);
}

/// Error history overlay.
pub fn render_error_history(f: &mut Frame, area: Rect, app: &AppState) {
    let popup = centered_rect(80, 70, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::negative())
        .title(format!(
            " Error History ({}) [Esc]close [j/k]scroll ",
            app.error_history.len()
        ))
        .title_style(theme::negative());

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if app.error_history.is_empty() {
        let text = Paragraph::new(Span::styled("No errors recorded.", theme::muted()));
        f.render_widget(text, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let start = app.error_scroll;
    let end = (start + visible_height).min(app.error_history.len());

    let mut lines: Vec<Line> = Vec::new();
    for i in start..end {
        let err = &app.error_history[i];
        let is_active = i == app.error_scroll;
        let style = if is_active {
            theme::negative().add_modifier(Modifier::BOLD)
        } else {
            theme::muted()
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", err.timestamp.format("%H:%M:%S")),
                theme::muted(),
            ),
            Span::styled(
                format!("[{}] ", err.category.label()),
                theme::warning(),
            ),
            Span::styled(&err.message, style),
        ]));

        if !err.context.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(&err.context, theme::muted()),
            ]));
        }
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);
}

/// Symbol search overlay.
pub fn render_search(f: &mut Frame, area: Rect, input: &str) {
    let popup = centered_rect(50, 20, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(" Add Symbol [Enter]add [Esc]cancel ")
        .title_style(theme::accent_bold());

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled("Enter ticker symbol:", theme::muted())),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", theme::accent()),
            Span::styled(input, theme::accent_bold()),
            Span::styled("_", theme::accent()),
        ]),
    ];

    let para = Paragraph::new(text);
    f.render_widget(para, inner);
}

/// Detail drill-down overlay for a leaderboard entry.
pub fn render_detail(f: &mut Frame, area: Rect, app: &AppState, idx: usize) {
    let popup = centered_rect(80, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(" Strategy Detail [Esc]close ")
        .title_style(theme::accent_bold());

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if idx >= app.results.entries.len() {
        let text = Paragraph::new(Span::styled("Entry not found.", theme::muted()));
        f.render_widget(text, inner);
        return;
    }

    let entry = &app.results.entries[idx];
    let m = &entry.metrics;
    let mut lines: Vec<Line> = Vec::new();

    // Composition
    lines.push(Line::from(Span::styled("Composition", theme::accent_bold())));
    metric_line(&mut lines, "Signal", &entry.signal_type);
    metric_line(&mut lines, "Position Manager", &entry.pm_type);
    metric_line(&mut lines, "Execution", &entry.exec_type);
    metric_line(&mut lines, "Filter", &entry.filter_type);
    metric_line(&mut lines, "Symbol", &entry.symbol);
    lines.push(Line::from(""));

    // Performance
    lines.push(Line::from(Span::styled("Performance", theme::accent_bold())));
    metric_num(&mut lines, "Sharpe", m.sharpe, false);
    metric_num(&mut lines, "CAGR", m.cagr * 100.0, true);
    metric_num(&mut lines, "Max Drawdown", m.max_drawdown * 100.0, true);
    metric_num(&mut lines, "Win Rate", m.win_rate * 100.0, true);
    metric_num(&mut lines, "Profit Factor", m.profit_factor, false);
    metric_num(&mut lines, "Sortino", m.sortino, false);
    metric_num(&mut lines, "Calmar", m.calmar, false);
    metric_num(&mut lines, "Total Return", m.total_return * 100.0, true);
    metric_line(&mut lines, "Trade Count", &m.trade_count.to_string());
    metric_line(&mut lines, "Max Consec Wins", &m.max_consecutive_wins.to_string());
    metric_line(&mut lines, "Max Consec Losses", &m.max_consecutive_losses.to_string());
    lines.push(Line::from(""));

    // Stickiness
    if let Some(stick) = &entry.stickiness {
        lines.push(Line::from(Span::styled("Stickiness", theme::accent_bold())));
        metric_num(&mut lines, "Median Hold (bars)", stick.median_holding_bars, false);
        metric_num(&mut lines, "P95 Hold (bars)", stick.p95_holding_bars, false);
        metric_num(&mut lines, "% Over 60 bars", stick.pct_over_60_bars * 100.0, true);
        metric_num(&mut lines, "% Over 120 bars", stick.pct_over_120_bars * 100.0, true);
        metric_num(&mut lines, "Exit Trigger Rate", stick.exit_trigger_rate * 100.0, true);
        metric_num(&mut lines, "Chase Ratio", stick.reference_chase_ratio, false);
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);
}

fn metric_line<'a>(lines: &mut Vec<Line<'a>>, label: &str, value: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("  {:>20}: ", label), theme::muted()),
        Span::styled(value.to_string(), theme::accent()),
    ]));
}

fn metric_num<'a>(lines: &mut Vec<Line<'a>>, label: &str, value: f64, pct: bool) {
    let display = if pct {
        format!("{value:.2}%")
    } else {
        format!("{value:.4}")
    };
    let style = theme::metric_color(value);
    lines.push(Line::from(vec![
        Span::styled(format!("  {:>20}: ", label), theme::muted()),
        Span::styled(display, style),
    ]));
}
