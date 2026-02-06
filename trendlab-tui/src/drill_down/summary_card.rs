//! Summary card overlay for strategy details
//!
//! Displays key metrics when a strategy is selected from the leaderboard.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use crate::theme::Theme;

/// Summary card data
#[derive(Debug, Clone)]
pub struct SummaryCardData {
    pub run_id: String,
    pub strategy_name: String,
    pub sharpe: f64,
    pub total_return: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub trade_count: usize,
    pub avg_trade_duration_days: f64,
    pub profit_factor: f64,
}

/// Summary card widget
pub struct SummaryCard<'a> {
    data: &'a SummaryCardData,
    theme: &'a Theme,
}

impl<'a> SummaryCard<'a> {
    pub fn new(data: &'a SummaryCardData, theme: &'a Theme) -> Self {
        Self { data, theme }
    }

    fn format_percentage(&self, value: f64) -> String {
        format!("{:+.2}%", value * 100.0)
    }

    fn format_metric_line(&self, label: &str, value: String, color: ratatui::style::Color) -> Line<'a> {
        Line::from(vec![
            Span::styled(
                format!("{:20}", label),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(value, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ])
    }
}

impl<'a> Widget for SummaryCard<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Create centered overlay (60% width, 70% height)
        let overlay_width = (area.width as f32 * 0.6) as u16;
        let overlay_height = (area.height as f32 * 0.7) as u16;
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;

        let overlay_area = Rect {
            x: area.x + x,
            y: area.y + y,
            width: overlay_width,
            height: overlay_height,
        };

        // Build content lines
        let lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                &self.data.strategy_name,
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]),
            Line::from(""),
            self.format_metric_line(
                "Sharpe Ratio:",
                format!("{:.2}", self.data.sharpe),
                self.theme.sharpe_color(self.data.sharpe),
            ),
            self.format_metric_line(
                "Total Return:",
                self.format_percentage(self.data.total_return),
                self.theme.pnl_color(self.data.total_return),
            ),
            self.format_metric_line(
                "Max Drawdown:",
                self.format_percentage(self.data.max_drawdown),
                self.theme.negative,
            ),
            self.format_metric_line(
                "Win Rate:",
                format!("{:.1}%", self.data.win_rate * 100.0),
                self.theme.win_rate_color(self.data.win_rate),
            ),
            self.format_metric_line(
                "Trade Count:",
                format!("{}", self.data.trade_count),
                self.theme.neutral,
            ),
            self.format_metric_line(
                "Avg Trade Duration:",
                format!("{:.1} days", self.data.avg_trade_duration_days),
                self.theme.muted,
            ),
            self.format_metric_line(
                "Profit Factor:",
                format!("{:.2}", self.data.profit_factor),
                if self.data.profit_factor >= 1.5 {
                    self.theme.positive
                } else {
                    self.theme.muted
                },
            ),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Press Enter to view trades | Esc to go back",
                Style::default()
                    .fg(self.theme.muted)
                    .add_modifier(Modifier::ITALIC),
            )]),
        ];

        let block = Block::default()
            .title(" Strategy Summary ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .style(Style::default().bg(self.theme.background));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        paragraph.render(overlay_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_card_data_creation() {
        let data = SummaryCardData {
            run_id: "run_1".to_string(),
            strategy_name: "MA Cross".to_string(),
            sharpe: 2.5,
            total_return: 0.45,
            max_drawdown: -0.12,
            win_rate: 0.64,
            trade_count: 25,
            avg_trade_duration_days: 8.5,
            profit_factor: 2.3,
        };

        assert_eq!(data.run_id, "run_1");
        assert_eq!(data.sharpe, 2.5);
    }

    #[test]
    fn test_summary_card_format_percentage() {
        let theme = Theme::default();
        let data = SummaryCardData {
            run_id: "run_1".to_string(),
            strategy_name: "Test".to_string(),
            sharpe: 2.5,
            total_return: 0.45,
            max_drawdown: -0.12,
            win_rate: 0.64,
            trade_count: 25,
            avg_trade_duration_days: 8.5,
            profit_factor: 2.3,
        };

        let card = SummaryCard::new(&data, &theme);
        assert_eq!(card.format_percentage(0.45), "+45.00%");
        assert_eq!(card.format_percentage(-0.12), "-12.00%");
    }
}
