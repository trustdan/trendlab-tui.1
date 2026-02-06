//! Leaderboard panel - shows top strategies ranked by performance
//!
//! Displays:
//! - Strategy name / run_id
//! - Sharpe ratio
//! - Total return
//! - Max drawdown
//! - Win rate
//! - Trade count

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, Widget},
};
use crate::theme::Theme;
use trendlab_runner::result::BacktestResult;
use trendlab_runner::FitnessMetric;

/// Leaderboard panel widget
pub struct LeaderboardPanel<'a> {
    results: &'a [&'a BacktestResult],
    selected_index: usize,
    theme: &'a Theme,
    fitness_metric: FitnessMetric,
    session_only: bool,
}

impl<'a> LeaderboardPanel<'a> {
    pub fn new(
        results: &'a [&'a BacktestResult],
        selected_index: usize,
        theme: &'a Theme,
        fitness_metric: FitnessMetric,
        session_only: bool,
    ) -> Self {
        Self {
            results,
            selected_index,
            theme,
            fitness_metric,
            session_only,
        }
    }

    fn metric_name(&self) -> &'static str {
        match self.fitness_metric {
            FitnessMetric::Sharpe => "Sharpe",
            FitnessMetric::Sortino => "Sortino",
            FitnessMetric::Calmar => "Calmar",
            FitnessMetric::TotalReturn => "Return",
            FitnessMetric::AnnualReturn => "Ann.Return",
            FitnessMetric::WinRate => "WinRate",
            FitnessMetric::ProfitFactor => "PF",
            FitnessMetric::Composite => "Composite",
        }
    }

    fn format_metric_value(&self, result: &BacktestResult) -> String {
        let val = self.fitness_metric.extract(result);
        match self.fitness_metric {
            FitnessMetric::TotalReturn | FitnessMetric::AnnualReturn | FitnessMetric::WinRate => {
                format!("{:+.2}%", val * 100.0)
            }
            _ => format!("{:.2}", val),
        }
    }

    fn format_percentage(&self, value: f64) -> String {
        format!("{:+.2}%", value * 100.0)
    }
}

impl<'a> Widget for LeaderboardPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let scope_label = if self.session_only { "Session" } else { "All-time" };
        let title = format!(
            " Strategy Leaderboard [{}] [{}] ",
            self.metric_name(),
            scope_label,
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .style(Style::default().bg(self.theme.background));

        let metric_col_name = self.metric_name();
        let header_names = ["Rank", "Run ID", metric_col_name, "Return", "Drawdown", "Win%", "Trades"];
        let header_cells = header_names
            .iter()
            .map(|h| {
                Cell::from(*h).style(
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
            });

        let header = Row::new(header_cells).height(1);

        let rows = self.results.iter().enumerate().map(|(i, result)| {
            let is_selected = i == self.selected_index;
            let style = if is_selected {
                Style::default()
                    .bg(self.theme.neutral)
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            let metric_val = self.fitness_metric.extract(result);
            let cells = vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(result.run_id.chars().take(12).collect::<String>()),
                Cell::from(self.format_metric_value(result))
                    .style(Style::default().fg(self.theme.sharpe_color(metric_val))),
                Cell::from(self.format_percentage(result.stats.total_return))
                    .style(Style::default().fg(self.theme.pnl_color(result.stats.total_return))),
                Cell::from(self.format_percentage(result.stats.max_drawdown))
                    .style(Style::default().fg(self.theme.negative)),
                Cell::from(format!("{:.1}%", result.stats.win_rate * 100.0))
                    .style(Style::default().fg(self.theme.win_rate_color(result.stats.win_rate))),
                Cell::from(format!("{}", result.stats.num_trades)),
            ];

            Row::new(cells).style(style).height(1)
        });

        let widths = [
            Constraint::Length(5),
            Constraint::Length(14),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(7),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .column_spacing(1);

        table.render(area, buf);

        // Help text at bottom
        let help_y = area.y + area.height.saturating_sub(2);
        let help_area = Rect {
            x: area.x + 2,
            y: help_y,
            width: area.width.saturating_sub(4),
            height: 1,
        };

        let help_text = Line::from(vec![
            Span::styled("↑/↓: ", Style::default().fg(self.theme.muted)),
            Span::styled("Select", Style::default().fg(self.theme.text_secondary)),
            Span::styled(" │ ", Style::default().fg(self.theme.muted)),
            Span::styled("Enter: ", Style::default().fg(self.theme.muted)),
            Span::styled("Drill down", Style::default().fg(self.theme.text_secondary)),
            Span::styled(" │ ", Style::default().fg(self.theme.muted)),
            Span::styled("f: ", Style::default().fg(self.theme.muted)),
            Span::styled("Metric", Style::default().fg(self.theme.text_secondary)),
            Span::styled(" │ ", Style::default().fg(self.theme.muted)),
            Span::styled("s: ", Style::default().fg(self.theme.muted)),
            Span::styled("Session", Style::default().fg(self.theme.text_secondary)),
            Span::styled(" │ ", Style::default().fg(self.theme.muted)),
            Span::styled("q: ", Style::default().fg(self.theme.muted)),
            Span::styled("Quit", Style::default().fg(self.theme.text_secondary)),
        ]);

        buf.set_line(help_area.x, help_area.y, &help_text, help_area.width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_result;

    #[test]
    fn test_leaderboard_panel_creation() {
        let theme = Theme::default();
        let result = create_test_result("run_1", 2.5);
        let results: Vec<&BacktestResult> = vec![&result];

        let panel = LeaderboardPanel::new(&results, 0, &theme, FitnessMetric::Sharpe, false);
        assert_eq!(panel.selected_index, 0);
    }

    #[test]
    fn test_format_percentage() {
        let theme = Theme::default();
        let results: Vec<&BacktestResult> = vec![];
        let panel = LeaderboardPanel::new(&results, 0, &theme, FitnessMetric::Sharpe, false);

        assert_eq!(panel.format_percentage(0.45), "+45.00%");
        assert_eq!(panel.format_percentage(-0.12), "-12.00%");
    }

    #[test]
    fn test_metric_name_display() {
        let theme = Theme::default();
        let results: Vec<&BacktestResult> = vec![];

        let panel = LeaderboardPanel::new(&results, 0, &theme, FitnessMetric::Sortino, false);
        assert_eq!(panel.metric_name(), "Sortino");

        let panel = LeaderboardPanel::new(&results, 0, &theme, FitnessMetric::WinRate, true);
        assert_eq!(panel.metric_name(), "WinRate");
        assert!(panel.session_only);
    }
}
