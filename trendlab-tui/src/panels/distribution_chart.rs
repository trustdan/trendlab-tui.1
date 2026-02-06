//! Distribution chart - horizontal box-and-whisker widget
//!
//! Renders a box plot with:
//! - Whiskers at p10 and p90
//! - Box from Q1 (p25) to Q3 (p75)
//! - Median marker
//! - Braille mini-histogram of all values

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use crate::theme::Theme;
use trendlab_runner::MetricDistribution;

/// Distribution chart widget (box-and-whisker)
pub struct DistributionChart<'a> {
    dist: &'a MetricDistribution,
    theme: &'a Theme,
}

impl<'a> DistributionChart<'a> {
    pub fn new(dist: &'a MetricDistribution, theme: &'a Theme) -> Self {
        Self { dist, theme }
    }
}

impl<'a> Widget for DistributionChart<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!(" {} Distribution ", self.dist.metric);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.neutral))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(area);
        block.render(area, buf);

        if self.dist.all_values.is_empty() || inner.width < 10 || inner.height < 3 {
            return;
        }

        let p10 = self.dist.get_percentile("p10").unwrap_or(self.dist.median);
        let p25 = self.dist.get_percentile("p25").unwrap_or(self.dist.median);
        let p75 = self.dist.get_percentile("p75").unwrap_or(self.dist.median);
        let p90 = self.dist.get_percentile("p90").unwrap_or(self.dist.median);
        let median = self.dist.median;

        let (min_val, max_val) = self.dist.range();
        let range = max_val - min_val;
        if range < 1e-12 {
            // All values identical
            buf.set_string(
                inner.x,
                inner.y,
                format!("All values = {:.4}", median),
                Style::default().fg(self.theme.muted),
            );
            return;
        }

        // Map value to x position in plot area
        let label_width: u16 = 8;
        let plot_left = inner.x + label_width;
        let plot_width = inner.width.saturating_sub(label_width);
        let val_to_x = |v: f64| -> u16 {
            let frac = (v - min_val) / range;
            plot_left + (frac * (plot_width.saturating_sub(1)) as f64).round() as u16
        };

        // Row 0: Percentile labels
        let label_y = inner.y;
        let labels = [
            (p10, "p10"),
            (p25, "Q1"),
            (median, "Med"),
            (p75, "Q3"),
            (p90, "p90"),
        ];
        for (val, lbl) in &labels {
            let x = val_to_x(*val);
            if x + lbl.len() as u16 <= inner.right() {
                buf.set_string(
                    x,
                    label_y,
                    lbl,
                    Style::default().fg(self.theme.text_secondary),
                );
            }
        }

        // Row 1: Box plot line
        let box_y = inner.y + 1;
        if box_y < inner.bottom() {
            let x_p10 = val_to_x(p10);
            let x_p25 = val_to_x(p25);
            let x_med = val_to_x(median);
            let x_p75 = val_to_x(p75);
            let x_p90 = val_to_x(p90);

            let whisker_style = Style::default().fg(self.theme.muted);
            let box_style = Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD);
            let median_style = Style::default()
                .fg(self.theme.warning)
                .add_modifier(Modifier::BOLD);

            // Left whisker: p10 to p25
            for x in x_p10..x_p25 {
                if x < inner.right() {
                    buf.set_string(x, box_y, "\u{2500}", whisker_style); // ─
                }
            }

            // Box: p25 to p75
            if x_p25 < inner.right() {
                buf.set_string(x_p25, box_y, "\u{251C}", box_style); // ├
            }
            for x in (x_p25 + 1)..x_p75 {
                if x < inner.right() {
                    buf.set_string(x, box_y, "\u{2550}", box_style); // ═
                }
            }
            if x_p75 < inner.right() {
                buf.set_string(x_p75, box_y, "\u{2524}", box_style); // ┤
            }

            // Median marker (overwrite)
            if x_med < inner.right() {
                buf.set_string(x_med, box_y, "\u{2502}", median_style); // │
            }

            // Right whisker: p75 to p90
            for x in (x_p75 + 1)..=x_p90 {
                if x < inner.right() {
                    buf.set_string(x, box_y, "\u{2500}", whisker_style); // ─
                }
            }
        }

        // Row 2: Value labels
        let val_y = inner.y + 2;
        if val_y < inner.bottom() {
            let left_label = format!("{:.2}", min_val);
            let right_label = format!("{:.2}", max_val);
            let med_label = format!("{:.2}", median);

            buf.set_string(
                plot_left,
                val_y,
                &left_label,
                Style::default().fg(self.theme.muted),
            );

            let x_med = val_to_x(median);
            if x_med + med_label.len() as u16 <= inner.right() {
                buf.set_string(
                    x_med,
                    val_y,
                    &med_label,
                    Style::default()
                        .fg(self.theme.warning)
                        .add_modifier(Modifier::BOLD),
                );
            }

            let right_x = inner.right().saturating_sub(right_label.len() as u16);
            buf.set_string(
                right_x,
                val_y,
                &right_label,
                Style::default().fg(self.theme.muted),
            );
        }

        // Row 3+: Mini histogram using braille
        if inner.height > 4 {
            let hist_y = inner.y + 4;
            let hist_height = inner.height.saturating_sub(5);
            if hist_height > 0 {
                let num_bins = plot_width as usize;
                let mut bins = vec![0usize; num_bins];
                for val in &self.dist.all_values {
                    let frac = (val - min_val) / range;
                    let bin = (frac * (num_bins - 1) as f64).round() as usize;
                    let bin = bin.min(num_bins - 1);
                    bins[bin] += 1;
                }

                let max_count = bins.iter().copied().max().unwrap_or(1).max(1);
                for (i, &count) in bins.iter().enumerate() {
                    let height =
                        (count as f64 / max_count as f64 * hist_height as f64).round() as u16;
                    let x = plot_left + i as u16;
                    if x >= inner.right() {
                        break;
                    }
                    for h in 0..height {
                        let y = hist_y + hist_height - 1 - h;
                        if y >= hist_y && y < inner.bottom() {
                            buf.set_string(
                                x,
                                y,
                                "\u{2587}", // ▇
                                Style::default().fg(self.theme.accent),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_test_dist() -> MetricDistribution {
        MetricDistribution::from_values("sharpe", &[1.0, 1.5, 2.0, 2.5, 3.0, 2.2, 1.8, 2.1, 1.9, 2.3])
    }

    #[test]
    fn test_distribution_chart_renders_without_panic() {
        let theme = Theme::default();
        let dist = make_test_dist();
        let panel = DistributionChart::new(&dist, &theme);

        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
    }

    #[test]
    fn test_distribution_chart_empty_values() {
        let theme = Theme::default();
        let dist = MetricDistribution {
            metric: "sharpe".to_string(),
            median: 0.0,
            mean: 0.0,
            iqr: 0.0,
            percentiles: HashMap::new(),
            all_values: vec![],
        };
        let panel = DistributionChart::new(&dist, &theme);

        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
        // Should not panic
    }

    #[test]
    fn test_distribution_chart_single_value() {
        let theme = Theme::default();
        let dist = MetricDistribution::from_values("sharpe", &[2.5]);
        let panel = DistributionChart::new(&dist, &theme);

        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("All values"));
    }
}
