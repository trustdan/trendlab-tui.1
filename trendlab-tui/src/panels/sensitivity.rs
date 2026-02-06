//! Sensitivity panel - cross-preset comparison table
//!
//! Shows metrics side-by-side for all execution presets:
//! Deterministic vs WorstCase vs BestCase vs PathMC
//! Color-coded relative to the Deterministic baseline.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, Widget},
};
use crate::theme::Theme;
use trendlab_runner::result::BacktestResult;
use std::collections::HashMap;

/// Sensitivity panel widget
pub struct SensitivityPanel<'a> {
    rerun_results: &'a HashMap<String, Box<BacktestResult>>,
    base_result: &'a BacktestResult,
    theme: &'a Theme,
}

impl<'a> SensitivityPanel<'a> {
    pub fn new(
        base_result: &'a BacktestResult,
        rerun_results: &'a HashMap<String, Box<BacktestResult>>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            rerun_results,
            base_result,
            theme,
        }
    }

    fn get_result(&self, preset: &str) -> Option<&BacktestResult> {
        if preset == "Deterministic" {
            Some(self.base_result)
        } else {
            let key = format!("{}:{}", self.base_result.run_id, preset);
            self.rerun_results.get(&key).map(|b| b.as_ref())
        }
    }

    fn format_cell(&self, value: Option<f64>, baseline: f64, is_pct: bool) -> (String, Style) {
        match value {
            None => (
                "[---]".to_string(),
                Style::default().fg(self.theme.muted),
            ),
            Some(v) => {
                let text = if is_pct {
                    format!("{:+.2}%", v * 100.0)
                } else {
                    format!("{:.2}", v)
                };

                let diff_pct = if baseline.abs() > 1e-9 {
                    ((v - baseline) / baseline.abs()) * 100.0
                } else {
                    0.0
                };

                let style = if diff_pct.abs() < 5.0 {
                    Style::default().fg(self.theme.muted)
                } else if diff_pct > 0.0 {
                    Style::default()
                        .fg(self.theme.positive)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(self.theme.negative)
                        .add_modifier(Modifier::BOLD)
                };

                (text, style)
            }
        }
    }

    fn format_dd_cell(&self, value: Option<f64>, baseline: f64) -> (String, Style) {
        match value {
            None => (
                "[---]".to_string(),
                Style::default().fg(self.theme.muted),
            ),
            Some(v) => {
                let text = format!("{:.2}%", v * 100.0);
                // For drawdown, less negative is better
                let diff = v - baseline;
                let style = if diff.abs() < 0.005 {
                    Style::default().fg(self.theme.muted)
                } else if diff < 0.0 {
                    // More drawdown = worse
                    Style::default()
                        .fg(self.theme.negative)
                        .add_modifier(Modifier::BOLD)
                } else {
                    // Less drawdown = better
                    Style::default()
                        .fg(self.theme.positive)
                        .add_modifier(Modifier::BOLD)
                };
                (text, style)
            }
        }
    }
}

impl<'a> Widget for SensitivityPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let presets = ["Deterministic", "WorstCase", "BestCase", "PathMC"];
        let results: Vec<Option<&BacktestResult>> =
            presets.iter().map(|p| self.get_result(p)).collect();

        type MetricDef = (&'static str, Box<dyn Fn(&BacktestResult) -> f64>, bool, bool);
        let metrics: Vec<MetricDef> = vec![
            ("Sharpe", Box::new(|r: &BacktestResult| r.stats.sharpe), false, false),
            ("Sortino", Box::new(|r: &BacktestResult| r.stats.sortino), false, false),
            ("Calmar", Box::new(|r: &BacktestResult| r.stats.calmar), false, false),
            ("Return", Box::new(|r: &BacktestResult| r.stats.total_return), true, false),
            ("Annual Ret", Box::new(|r: &BacktestResult| r.stats.annual_return), true, false),
            ("Max DD", Box::new(|r: &BacktestResult| r.stats.max_drawdown), false, true),
            ("Win Rate", Box::new(|r: &BacktestResult| r.stats.win_rate), true, false),
            ("Profit F.", Box::new(|r: &BacktestResult| r.stats.profit_factor), false, false),
            ("Trades", Box::new(|r: &BacktestResult| r.stats.num_trades as f64), false, false),
        ];

        // Header row
        let header_cells = [
            Cell::from("Metric").style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Determin.").style(
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("WorstCase").style(
                Style::default()
                    .fg(self.theme.negative)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("BestCase").style(
                Style::default()
                    .fg(self.theme.positive)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("PathMC").style(
                Style::default()
                    .fg(self.theme.neutral)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        let header = Row::new(header_cells).height(1);

        // Data rows
        let rows: Vec<Row> = metrics
            .iter()
            .map(|(name, extract, is_pct, is_dd)| {
                let baseline = extract(self.base_result);
                let mut cells = vec![Cell::from(*name).style(
                    Style::default().fg(self.theme.text_secondary),
                )];

                for result in &results {
                    let value = result.map(extract);
                    let (text, style) = if *is_dd {
                        self.format_dd_cell(value, baseline)
                    } else {
                        self.format_cell(value, baseline, *is_pct)
                    };
                    cells.push(Cell::from(text).style(style));
                }

                Row::new(cells).height(1)
            })
            .collect();

        let widths = [
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
        ];

        let complete_count = results.iter().filter(|r| r.is_some()).count();
        let title = format!(
            " Sensitivity Analysis [{}/4 presets] ",
            complete_count
        );

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.theme.neutral))
                    .style(Style::default().bg(self.theme.background)),
            )
            .column_spacing(1);

        table.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_result;

    #[test]
    fn test_sensitivity_panel_with_all_presets() {
        let theme = Theme::default();
        let base = create_test_result("run_1", 2.5);

        let mut rerun_results: HashMap<String, Box<BacktestResult>> = HashMap::new();
        let mut worst = create_test_result("run_1_worst", 1.8);
        worst.stats.total_return = 0.20;
        rerun_results.insert("run_1:WorstCase".to_string(), Box::new(worst));

        let mut best = create_test_result("run_1_best", 3.0);
        best.stats.total_return = 0.55;
        rerun_results.insert("run_1:BestCase".to_string(), Box::new(best));

        let panel = SensitivityPanel::new(&base, &rerun_results, &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("Sharpe"));
        assert!(content.contains("Sensitivity Analysis"));
        // PathMC not run → should show [---]
        assert!(content.contains("[---]"));
    }

    #[test]
    fn test_sensitivity_panel_empty_reruns() {
        let theme = Theme::default();
        let base = create_test_result("run_1", 2.5);
        let rerun_results: HashMap<String, Box<BacktestResult>> = HashMap::new();

        let panel = SensitivityPanel::new(&base, &rerun_results, &theme);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
        // Should render without panic
    }
}
