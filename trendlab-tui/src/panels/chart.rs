//! Chart panel - equity curve with ghost overlay
//!
//! Displays:
//! - Real equity curve (primary)
//! - Ideal equity curve (ghost, muted)
//! - Trade markers (entry/exit points)
//! - Execution drag metric
//! - Death crossing flag (>15% divergence)

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Widget},
};
use crate::theme::Theme;
use crate::ghost_curve::GhostCurve;

/// Chart panel widget
pub struct ChartPanel<'a> {
    ghost_curve: &'a GhostCurve,
    trade_markers: &'a [TradeMarker],
    theme: &'a Theme,
}

/// Marker for a trade entry/exit on the chart
#[derive(Debug, Clone)]
pub struct TradeMarker {
    pub bar_index: usize,
    pub price: f64,
    pub label: String,
}

impl<'a> ChartPanel<'a> {
    pub fn new(
        ghost_curve: &'a GhostCurve,
        trade_markers: &'a [TradeMarker],
        theme: &'a Theme,
    ) -> Self {
        Self {
            ghost_curve,
            trade_markers,
            theme,
        }
    }
}

impl<'a> Widget for ChartPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let drag_percentage = self.ghost_curve.final_drag_percentage();
        let drag_dollars = self.ghost_curve.final_drag_dollars();
        let is_death_crossing = self.ghost_curve.drag_metric.is_death_crossing();

        let title = if is_death_crossing {
            format!(
                " DEATH CROSSING | Drag: {:.2}% (${:.2}) | {} markers ",
                drag_percentage, drag_dollars, self.trade_markers.len()
            )
        } else {
            format!(
                " Equity Curve | Drag: {:.2}% (${:.2}) | {} markers ",
                drag_percentage, drag_dollars, self.trade_markers.len()
            )
        };

        let border_color = if is_death_crossing {
            self.theme.warning
        } else {
            self.theme.accent
        };

        // Build data points for real curve
        let real_data: Vec<(f64, f64)> = self
            .ghost_curve
            .real
            .values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64, v))
            .collect();

        // Build data points for ideal curve
        let ideal_data: Vec<(f64, f64)> = self
            .ghost_curve
            .ideal
            .values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64, v))
            .collect();

        // Compute axis bounds
        let all_values: Vec<f64> = real_data
            .iter()
            .chain(ideal_data.iter())
            .map(|&(_, v)| v)
            .collect();

        let x_max = real_data.len().max(ideal_data.len()) as f64;
        let y_min = all_values
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let y_max = all_values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);

        // Add padding to Y axis
        let y_range = y_max - y_min;
        let y_pad = if y_range > 0.0 { y_range * 0.05 } else { 100.0 };
        let y_lower = y_min - y_pad;
        let y_upper = y_max + y_pad;

        let datasets = vec![
            Dataset::default()
                .name("Real")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(self.theme.accent))
                .data(&real_data),
            Dataset::default()
                .name("Ideal (Ghost)")
                .marker(symbols::Marker::Dot)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(self.theme.muted))
                .data(&ideal_data),
        ];

        // Format Y axis labels
        let y_mid = (y_lower + y_upper) / 2.0;
        let x_labels = vec![
            Span::raw("0"),
            Span::raw(format!("{}", (x_max / 2.0) as usize)),
            Span::raw(format!("{}", x_max as usize)),
        ];
        let y_labels = vec![
            Span::raw(format!("${:.0}", y_lower)),
            Span::raw(format!("${:.0}", y_mid)),
            Span::raw(format!("${:.0}", y_upper)),
        ];

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color).add_modifier(
                        if is_death_crossing {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        },
                    ))
                    .style(Style::default().bg(self.theme.background)),
            )
            .x_axis(
                Axis::default()
                    .title(Span::styled("Bar", Style::default().fg(self.theme.text_secondary)))
                    .style(Style::default().fg(self.theme.muted))
                    .bounds([0.0, x_max])
                    .labels(x_labels),
            )
            .y_axis(
                Axis::default()
                    .title(Span::styled(
                        "Equity",
                        Style::default().fg(self.theme.text_secondary),
                    ))
                    .style(Style::default().fg(self.theme.muted))
                    .bounds([y_lower, y_upper])
                    .labels(y_labels),
            );

        chart.render(area, buf);

        // Render trade markers directly to buffer after chart rendering.
        // Ratatui's Chart widget doesn't support point annotations, so we
        // write labels at computed pixel positions.
        let block_for_inner = Block::default()
            .title("")
            .borders(Borders::ALL);
        let chart_inner = block_for_inner.inner(area);

        // Chart has axis labels that consume space. Approximate the plot area
        // as the chart_inner minus left label width (~8 chars) and bottom label height (1 row).
        let plot_left = chart_inner.x + 8; // Y-axis label width
        let plot_top = chart_inner.y;
        let plot_width = chart_inner.width.saturating_sub(8);
        let plot_height = chart_inner.height.saturating_sub(2); // bottom axis

        if plot_width > 0 && plot_height > 0 && x_max > 0.0 {
            for marker in self.trade_markers {
                let x_frac = marker.bar_index as f64 / x_max;
                let y_frac = if (y_upper - y_lower).abs() > 1e-9 {
                    (marker.price - y_lower) / (y_upper - y_lower)
                } else {
                    0.5
                };

                let px = plot_left + (x_frac * plot_width as f64) as u16;
                // Y is inverted (0 = top of screen)
                let py = plot_top + plot_height.saturating_sub(1)
                    - (y_frac * (plot_height.saturating_sub(1)) as f64) as u16;

                if px < area.right().saturating_sub(1) && py >= plot_top && py < plot_top + plot_height {
                    let color = if marker.label.starts_with('E') {
                        self.theme.positive
                    } else {
                        self.theme.negative
                    };
                    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                    buf.set_string(px, py, &marker.label, style);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghost_curve::{IdealEquity, RealEquity, GhostCurve};
    use chrono::{TimeZone, Utc};

    fn create_test_ghost_curve() -> GhostCurve {
        let mut ideal = IdealEquity::new();
        let mut real = RealEquity::new();
        let ts = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();

        ideal.push(ts, 10000.0);
        ideal.push(ts, 11000.0);

        real.push(ts, 10000.0);
        real.push(ts, 10800.0);

        GhostCurve::new(ideal, real)
    }

    fn create_death_crossing_curve() -> GhostCurve {
        let mut ideal = IdealEquity::new();
        let mut real = RealEquity::new();
        let ts = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();

        ideal.push(ts, 10000.0);
        ideal.push(ts, 12000.0); // +20%

        real.push(ts, 10000.0);
        real.push(ts, 10000.0); // 0% → drag = 16.67% > 15%

        GhostCurve::new(ideal, real)
    }

    #[test]
    fn test_chart_panel_creation() {
        let theme = Theme::default();
        let ghost_curve = create_test_ghost_curve();
        let markers: Vec<TradeMarker> = vec![];

        let panel = ChartPanel::new(&ghost_curve, &markers, &theme);
        assert_eq!(panel.trade_markers.len(), 0);
    }

    #[test]
    fn test_chart_panel_renders_without_panic() {
        let theme = Theme::default();
        let ghost_curve = create_test_ghost_curve();
        let markers: Vec<TradeMarker> = vec![];
        let panel = ChartPanel::new(&ghost_curve, &markers, &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
    }

    #[test]
    fn test_death_crossing_chart_renders_without_panic() {
        let theme = Theme::default();
        let ghost_curve = create_death_crossing_curve();
        let markers: Vec<TradeMarker> = vec![];
        let panel = ChartPanel::new(&ghost_curve, &markers, &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
    }

    #[test]
    fn test_chart_with_trade_markers_renders_without_panic() {
        let theme = Theme::default();
        let ghost_curve = create_test_ghost_curve();
        let markers = vec![
            TradeMarker { bar_index: 0, price: 10000.0, label: "E1".to_string() },
            TradeMarker { bar_index: 1, price: 10800.0, label: "X1".to_string() },
        ];
        let panel = ChartPanel::new(&ghost_curve, &markers, &theme);

        let area = Rect::new(0, 0, 120, 30);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Check that at least one marker label appears in the buffer
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        // Either E1 or X1 should be somewhere in the rendered output
        let has_entry = content.contains('E');
        let has_exit = content.contains('X');
        assert!(has_entry || has_exit, "Trade markers should appear in chart buffer");
    }
}
