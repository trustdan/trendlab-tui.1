//! Trade tape panel - list of all trades with details
//!
//! Displays:
//! - Trade ID
//! - Symbol
//! - Direction (Long/Short)
//! - Entry/exit dates
//! - PnL
//! - Duration

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, Widget},
};
use crate::theme::Theme;

/// Trade record for display
#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub trade_id: String,
    pub symbol: String,
    pub direction: String, // "Long" or "Short"
    pub entry_date: String,
    pub exit_date: String,
    pub pnl: f64,
    pub duration_days: u32,
    /// Signal that triggered this trade (e.g. "Long", "Short")
    pub signal_intent: Option<String>,
    /// Order type used (e.g. "Market(MOO)", "StopMarket(175.00)")
    pub order_type: Option<String>,
    /// Fill context (e.g. "Filled at open $100.00")
    pub fill_context: Option<String>,
}

/// Trade tape panel widget
pub struct TradeTapePanel<'a> {
    trades: &'a [TradeRecord],
    selected_index: usize,
    theme: &'a Theme,
}

impl<'a> TradeTapePanel<'a> {
    pub fn new(trades: &'a [TradeRecord], selected_index: usize, theme: &'a Theme) -> Self {
        Self {
            trades,
            selected_index,
            theme,
        }
    }

    fn format_pnl(&self, pnl: f64) -> String {
        format!("${:+.2}", pnl)
    }
}

impl<'a> Widget for TradeTapePanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" Trade Tape ({} trades) ", self.trades.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .style(Style::default().bg(self.theme.background));

        let header_cells = ["ID", "Symbol", "Dir", "Entry", "Exit", "PnL", "Days", "Signal", "Order", "Fill"]
            .iter()
            .map(|h| {
                Cell::from(*h).style(
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
            });

        let header = Row::new(header_cells).height(1);

        let rows = self.trades.iter().enumerate().map(|(i, trade)| {
            let is_selected = i == self.selected_index;
            let style = if is_selected {
                Style::default()
                    .bg(self.theme.neutral)
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            let direction_color = self.theme.signal_color(&trade.direction);

            let cells = vec![
                Cell::from(trade.trade_id.chars().take(8).collect::<String>()),
                Cell::from(trade.symbol.clone()),
                Cell::from(trade.direction.clone()).style(Style::default().fg(direction_color)),
                Cell::from(trade.entry_date.clone()),
                Cell::from(trade.exit_date.clone()),
                Cell::from(self.format_pnl(trade.pnl))
                    .style(Style::default().fg(self.theme.pnl_color(trade.pnl))),
                Cell::from(format!("{}", trade.duration_days)),
                Cell::from(trade.signal_intent.as_deref().unwrap_or("-").to_string()),
                Cell::from(trade.order_type.as_deref().unwrap_or("-").to_string()),
                Cell::from(trade.fill_context.as_deref().unwrap_or("-").to_string()),
            ];

            Row::new(cells).style(style).height(1)
        });

        let widths = [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(18),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .column_spacing(1);

        table.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_tape_panel_creation() {
        let theme = Theme::default();
        let trades = vec![TradeRecord {
            trade_id: "trade_1".to_string(),
            symbol: "AAPL".to_string(),
            direction: "Long".to_string(),
            entry_date: "2023-01-05".to_string(),
            exit_date: "2023-01-20".to_string(),
            pnl: 1250.0,
            duration_days: 15,
            signal_intent: Some("Long".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $150.00".to_string()),
        }];

        let panel = TradeTapePanel::new(&trades, 0, &theme);
        assert_eq!(panel.selected_index, 0);
    }

    #[test]
    fn test_format_pnl() {
        let theme = Theme::default();
        let trades = vec![];
        let panel = TradeTapePanel::new(&trades, 0, &theme);

        assert_eq!(panel.format_pnl(1250.0), "$+1250.00");
        assert_eq!(panel.format_pnl(-350.0), "$-350.00");
    }
}
