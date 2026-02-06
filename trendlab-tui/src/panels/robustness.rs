//! Robustness ladder visualization panel
//!
//! Three vertical sections:
//! 1. Ladder progress: row per level with pacman bar, status, stability score
//! 2. Stability detail: for selected level — metric, median, IQR, penalty, score
//! 3. Distribution: box-and-whisker for selected level's primary metric

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Widget},
};
use crate::theme::Theme;
use crate::panels::distribution_chart::DistributionChart;
use trendlab_runner::LevelResult;

/// Robustness panel widget
pub struct RobustnessPanel<'a> {
    levels: &'a [LevelResult],
    selected_level: usize,
    theme: &'a Theme,
}

impl<'a> RobustnessPanel<'a> {
    pub fn new(levels: &'a [LevelResult], selected_level: usize, theme: &'a Theme) -> Self {
        Self {
            levels,
            selected_level,
            theme,
        }
    }

    fn pacman_bar(&self, level_idx: usize, total_levels: usize) -> String {
        let pellet_count = 16;
        let progress = if total_levels > 0 {
            ((level_idx + 1) as f64 / total_levels as f64 * pellet_count as f64).round() as usize
        } else {
            0
        };
        let progress = progress.min(pellet_count);
        let before = ".".repeat(progress.saturating_sub(1));
        let after = "\u{00B7}".repeat(pellet_count.saturating_sub(progress));
        if progress > 0 {
            format!("[{}\u{1D5E7}{}]", before, after)
        } else {
            format!("[\u{1D5E7}{}]", after)
        }
    }

    fn status_text(&self, level: &LevelResult) -> (String, Style) {
        if level.promoted {
            (
                "PROMOTED".to_string(),
                Style::default()
                    .fg(self.theme.positive)
                    .add_modifier(Modifier::BOLD),
            )
        } else if let Some(reason) = &level.rejection_reason {
            (
                format!("REJECTED: {}", reason),
                Style::default()
                    .fg(self.theme.negative)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                "RUNNING".to_string(),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::ITALIC),
            )
        }
    }
}

impl<'a> Widget for RobustnessPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let promoted_count = self.levels.iter().filter(|l| l.promoted).count();
        let rejected_count = self
            .levels
            .iter()
            .filter(|l| !l.promoted && l.rejection_reason.is_some())
            .count();

        let title = format!(
            " Robustness Ladder | {} levels | {} promoted, {} rejected ",
            self.levels.len(),
            promoted_count,
            rejected_count,
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.neutral))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(area);
        block.render(area, buf);

        if self.levels.is_empty() || inner.height < 5 {
            return;
        }

        // Section 1: Ladder table (top half)
        let ladder_height = (self.levels.len() as u16 + 2).min(inner.height / 2);
        let ladder_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: ladder_height,
        };

        let header = Row::new(vec![
            Cell::from("Level").style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Progress").style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Status").style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Score").style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Trades").style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .height(1);

        let total = self.levels.len();
        let rows: Vec<Row> = self
            .levels
            .iter()
            .enumerate()
            .map(|(idx, level)| {
                let is_selected = idx == self.selected_level;
                let marker = if is_selected { ">" } else { " " };
                let name_style = if is_selected {
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                let bar = self.pacman_bar(idx, total);
                let (status_text, status_style) = self.status_text(level);
                let score_str = format!("{:.3}", level.stability_score.score);
                let score_style = if level.stability_score.score > 1.0 {
                    Style::default().fg(self.theme.positive)
                } else if level.stability_score.score > 0.5 {
                    Style::default().fg(self.theme.text_primary)
                } else {
                    Style::default().fg(self.theme.negative)
                };

                Row::new(vec![
                    Cell::from(format!("{} {}", marker, level.level_name)).style(name_style),
                    Cell::from(bar).style(Style::default().fg(self.theme.muted)),
                    Cell::from(status_text).style(status_style),
                    Cell::from(score_str).style(score_style),
                    Cell::from(level.trade_count.to_string())
                        .style(Style::default().fg(self.theme.text_secondary)),
                ])
                .height(1)
            })
            .collect();

        let widths = [
            Constraint::Length(16),
            Constraint::Length(20),
            Constraint::Length(25),
            Constraint::Length(8),
            Constraint::Length(8),
        ];

        let table = Table::new(rows, widths).header(header).column_spacing(1);
        table.render(ladder_area, buf);

        // Section 2: Stability detail for selected level
        let detail_y = inner.y + ladder_height + 1;
        let remaining = inner.height.saturating_sub(ladder_height + 1);
        if remaining < 4 {
            return;
        }

        if let Some(level) = self.levels.get(self.selected_level) {
            let detail_height = 5.min(remaining / 2);
            let detail_area = Rect {
                x: inner.x,
                y: detail_y,
                width: inner.width,
                height: detail_height,
            };

            let ss = &level.stability_score;
            let detail_lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!("  Stability Detail: {} ", level.level_name),
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Metric: ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(&ss.metric, Style::default().fg(self.theme.text_primary)),
                    Span::styled("  Median: ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(
                        format!("{:.4}", ss.median),
                        Style::default().fg(self.theme.text_primary),
                    ),
                    Span::styled("  IQR: ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(
                        format!("{:.4}", ss.iqr),
                        Style::default().fg(self.theme.text_primary),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Penalty: ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(
                        format!("{:.2}", ss.penalty_factor),
                        Style::default().fg(self.theme.text_primary),
                    ),
                    Span::styled("  Score: ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(
                        format!("{:.4} (median - {:.2} * IQR)", ss.score, ss.penalty_factor),
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];

            let paragraph = Paragraph::new(detail_lines);
            paragraph.render(detail_area, buf);

            // Section 3: Distribution chart for primary metric
            let dist_y = detail_y + detail_height;
            let dist_height = remaining.saturating_sub(detail_height);
            if dist_height >= 4 {
                if let Some(dist) = level.distributions.first() {
                    let dist_area = Rect {
                        x: inner.x,
                        y: dist_y,
                        width: inner.width,
                        height: dist_height,
                    };
                    let chart = DistributionChart::new(dist, self.theme);
                    chart.render(dist_area, buf);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trendlab_runner::{StabilityScore, MetricDistribution};
    use trendlab_runner::config::*;
    use chrono::NaiveDate;

    fn make_test_config() -> RunConfig {
        RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::BuyAndHold,
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2020, 12, 31).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    fn make_test_levels() -> Vec<LevelResult> {
        let dist = MetricDistribution::from_values(
            "sharpe",
            &[1.0, 1.5, 2.0, 2.5, 3.0, 1.8, 2.2, 1.9],
        );
        vec![
            LevelResult {
                level_name: "CheapPass".to_string(),
                config: make_test_config(),
                stability_score: StabilityScore::compute("sharpe", &[1.0, 1.5, 2.0, 2.5, 3.0], 0.5),
                distributions: vec![dist.clone()],
                trade_count: 42,
                promoted: true,
                rejection_reason: None,
            },
            LevelResult {
                level_name: "WalkForward".to_string(),
                config: make_test_config(),
                stability_score: StabilityScore::compute("sharpe", &[0.8, 1.2, 0.5, 1.0], 0.5),
                distributions: vec![dist.clone()],
                trade_count: 38,
                promoted: true,
                rejection_reason: None,
            },
            LevelResult {
                level_name: "ExecutionMC".to_string(),
                config: make_test_config(),
                stability_score: StabilityScore::compute("sharpe", &[0.3, 0.4, 0.2, 0.5], 0.5),
                distributions: vec![dist],
                trade_count: 25,
                promoted: false,
                rejection_reason: Some("stability_too_low".to_string()),
            },
        ]
    }

    #[test]
    fn test_robustness_panel_renders() {
        let theme = Theme::default();
        let levels = make_test_levels();
        let panel = RobustnessPanel::new(&levels, 0, &theme);

        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("Robustness Ladder"));
        assert!(content.contains("CheapPass"));
        assert!(content.contains("PROMOTED"));
        assert!(content.contains("REJECTED"));
    }

    #[test]
    fn test_robustness_panel_empty_levels() {
        let theme = Theme::default();
        let levels: Vec<LevelResult> = vec![];
        let panel = RobustnessPanel::new(&levels, 0, &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
        // Should not panic
    }

    #[test]
    fn test_robustness_panel_with_distribution() {
        let theme = Theme::default();
        let levels = make_test_levels();
        let panel = RobustnessPanel::new(&levels, 0, &theme);

        // Use a taller area to ensure distribution chart renders
        let area = Rect::new(0, 0, 100, 40);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("Stability Detail"));
    }
}
