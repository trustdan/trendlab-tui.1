//! Panel 5 — Chart: MVP equity curve line chart.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};

use crate::app::AppState;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let chart_state = &app.chart;

    match &chart_state.equity_curve {
        Some(curve) if !curve.is_empty() => render_chart(f, area, curve, &chart_state.label),
        _ => render_empty(f, area),
    }
}

fn render_empty(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "Select a result from Panel 4 to display its equity curve.",
            theme::muted(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Navigate to Results (press 4), select an entry, and press Enter.",
            theme::muted(),
        )),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn render_chart(f: &mut Frame, area: Rect, curve: &[f64], label: &str) {
    let min_y = curve
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let max_y = curve
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    let padding = (max_y - min_y).abs() * 0.05;
    let y_min = min_y - padding;
    let y_max = max_y + padding;
    let x_max = curve.len().saturating_sub(1) as f64;

    // Convert to (x, y) data points
    let data: Vec<(f64, f64)> = curve
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let dataset = Dataset::default()
        .name(label)
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(theme::ACCENT))
        .graph_type(GraphType::Line)
        .data(&data);

    let chart = Chart::new(vec![dataset])
        .x_axis(
            Axis::default()
                .title(Span::styled("Bars", theme::muted()))
                .style(theme::muted())
                .bounds([0.0, x_max.max(1.0)])
                .labels(vec![
                    Span::styled("0", theme::muted()),
                    Span::styled(format!("{}", curve.len()), theme::muted()),
                ]),
        )
        .y_axis(
            Axis::default()
                .title(Span::styled("Equity", theme::muted()))
                .style(theme::muted())
                .bounds([y_min, y_max])
                .labels(vec![
                    Span::styled(format!("{:.0}", y_min), theme::muted()),
                    Span::styled(format!("{:.0}", y_max), theme::muted()),
                ]),
        );

    f.render_widget(chart, area);
}
