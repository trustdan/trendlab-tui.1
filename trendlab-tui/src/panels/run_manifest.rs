//! Run manifest viewer - displays the full RunConfig for a result
//!
//! Shows: signal generator, order policy, position sizer,
//! execution config, universe, date range, initial capital.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::theme::Theme;
use trendlab_runner::config::*;

fn format_dollars(amount: f64) -> String {
    let s = format!("{:.2}", amount);
    let parts: Vec<&str> = s.split('.').collect();
    let int_part = parts[0];
    let dec_part = parts[1];
    let chars: Vec<char> = int_part.chars().collect();
    let mut result = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }
    format!("${}.{}", result, dec_part)
}

/// Run manifest viewer widget
pub struct RunManifestPanel<'a> {
    config: &'a RunConfig,
    run_id: &'a str,
    theme: &'a Theme,
}

impl<'a> RunManifestPanel<'a> {
    pub fn new(config: &'a RunConfig, run_id: &'a str, theme: &'a Theme) -> Self {
        Self {
            config,
            run_id,
            theme,
        }
    }

    fn section_header(&self, label: &str) -> Line<'a> {
        Line::from(Span::styled(
            format!("--- {} ---", label),
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    }

    fn key_value(&self, key: &str, value: String) -> Line<'a> {
        Line::from(vec![
            Span::styled(
                format!("  {}: ", key),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                value,
                Style::default().fg(self.theme.text_primary),
            ),
        ])
    }

    fn format_signal_generator(&self) -> Vec<Line<'a>> {
        let mut lines = vec![self.section_header("Signal Generator")];
        match &self.config.strategy.signal_generator {
            SignalGeneratorConfig::MaCrossover { short_period, long_period } => {
                lines.push(self.key_value("Type", "MA Crossover".to_string()));
                lines.push(self.key_value("Short Period", short_period.to_string()));
                lines.push(self.key_value("Long Period", long_period.to_string()));
            }
            SignalGeneratorConfig::BuyAndHold => {
                lines.push(self.key_value("Type", "Buy & Hold".to_string()));
            }
            SignalGeneratorConfig::Custom { name, params } => {
                lines.push(self.key_value("Type", format!("Custom: {}", name)));
                for (k, v) in params {
                    lines.push(self.key_value(&format!("  {}", k), v.to_string()));
                }
            }
        }
        lines
    }

    fn format_order_policy(&self) -> Vec<Line<'a>> {
        let mut lines = vec![self.section_header("Order Policy")];
        match &self.config.strategy.order_policy {
            OrderPolicyConfig::Simple => {
                lines.push(self.key_value("Type", "Simple (Market @ Next Open)".to_string()));
            }
            OrderPolicyConfig::Custom { name, .. } => {
                lines.push(self.key_value("Type", format!("Custom: {}", name)));
            }
        }
        lines
    }

    fn format_position_sizer(&self) -> Vec<Line<'a>> {
        let mut lines = vec![self.section_header("Position Sizer")];
        match &self.config.strategy.position_sizer {
            PositionSizerConfig::FixedDollar { amount } => {
                lines.push(self.key_value("Type", "Fixed Dollar".to_string()));
                lines.push(self.key_value("Amount", format!("${:.2}", amount)));
            }
            PositionSizerConfig::FixedShares { shares } => {
                lines.push(self.key_value("Type", "Fixed Shares".to_string()));
                lines.push(self.key_value("Shares", shares.to_string()));
            }
            PositionSizerConfig::PercentEquity { percent } => {
                lines.push(self.key_value("Type", "Percent Equity".to_string()));
                lines.push(self.key_value("Percent", format!("{:.1}%", percent)));
            }
            PositionSizerConfig::Custom { name, .. } => {
                lines.push(self.key_value("Type", format!("Custom: {}", name)));
            }
        }
        lines
    }

    fn format_execution(&self) -> Vec<Line<'a>> {
        let mut lines = vec![self.section_header("Execution Model")];
        let exec = &self.config.execution;

        match &exec.slippage {
            SlippageConfig::FixedBps { bps } => {
                lines.push(self.key_value("Slippage", format!("{} bps", bps)));
            }
            SlippageConfig::Percentage { percent } => {
                lines.push(self.key_value("Slippage", format!("{:.2}%", percent)));
            }
            SlippageConfig::None => {
                lines.push(self.key_value("Slippage", "None".to_string()));
            }
        }

        match &exec.commission {
            CommissionConfig::PerTrade { amount } => {
                lines.push(self.key_value("Commission", format!("${:.2}/trade", amount)));
            }
            CommissionConfig::PerShare { amount } => {
                lines.push(self.key_value("Commission", format!("${:.4}/share", amount)));
            }
            CommissionConfig::Percentage { percent } => {
                lines.push(self.key_value("Commission", format!("{:.3}%", percent)));
            }
            CommissionConfig::None => {
                lines.push(self.key_value("Commission", "None".to_string()));
            }
        }

        let policy = match exec.intrabar_policy {
            IntrabarPolicy::WorstCase => "WorstCase (conservative)",
            IntrabarPolicy::BestCase => "BestCase (optimistic)",
            IntrabarPolicy::OhlcOrder => "OHLC Order (heuristic)",
        };
        lines.push(self.key_value("Intrabar Policy", policy.to_string()));
        lines
    }

    fn format_backtest_params(&self) -> Vec<Line<'a>> {
        let mut lines = vec![self.section_header("Backtest Parameters")];
        lines.push(self.key_value(
            "Date Range",
            format!("{} to {}", self.config.start_date, self.config.end_date),
        ));
        lines.push(self.key_value(
            "Universe",
            self.config.universe.join(", "),
        ));
        lines.push(self.key_value(
            "Initial Capital",
            format_dollars(self.config.initial_capital),
        ));
        lines
    }
}

impl<'a> Widget for RunManifestPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!(" Run Manifest: {} ", self.run_id);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.neutral))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        lines.extend(self.format_signal_generator());
        lines.push(Line::from(""));
        lines.extend(self.format_order_policy());
        lines.push(Line::from(""));
        lines.extend(self.format_position_sizer());
        lines.push(Line::from(""));
        lines.extend(self.format_execution());
        lines.push(Line::from(""));
        lines.extend(self.format_backtest_params());
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "Press Esc to go back",
            Style::default()
                .fg(self.theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));

        let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn make_test_config() -> RunConfig {
        RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::MaCrossover {
                    short_period: 10,
                    long_period: 50,
                },
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2020, 12, 31).unwrap(),
            universe: vec!["SPY".to_string(), "QQQ".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    #[test]
    fn test_run_manifest_renders_all_sections() {
        let theme = Theme::default();
        let config = make_test_config();
        let panel = RunManifestPanel::new(&config, "test_run_123", &theme);

        let area = Rect::new(0, 0, 80, 30);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }

        assert!(content.contains("Signal Generator"));
        assert!(content.contains("MA Crossover"));
        assert!(content.contains("Order Policy"));
        assert!(content.contains("Position Sizer"));
        assert!(content.contains("Execution Model"));
        assert!(content.contains("Backtest Parameters"));
        assert!(content.contains("SPY"));
    }

    #[test]
    fn test_run_manifest_buy_and_hold() {
        let theme = Theme::default();
        let mut config = make_test_config();
        config.strategy.signal_generator = SignalGeneratorConfig::BuyAndHold;
        let panel = RunManifestPanel::new(&config, "bah_run", &theme);

        let area = Rect::new(0, 0, 80, 30);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("Buy & Hold"));
    }

    #[test]
    fn test_run_manifest_custom_signal() {
        let theme = Theme::default();
        let mut config = make_test_config();
        config.strategy.signal_generator = SignalGeneratorConfig::Custom {
            name: "MeanReversion".to_string(),
            params: {
                let mut m = HashMap::new();
                m.insert("lookback".to_string(), serde_json::json!(20));
                m
            },
        };
        let panel = RunManifestPanel::new(&config, "custom_run", &theme);

        let area = Rect::new(0, 0, 80, 30);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("MeanReversion"));
    }
}
