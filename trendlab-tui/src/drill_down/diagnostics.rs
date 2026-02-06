//! Trade diagnostics view
//!
//! Shows detailed execution information for a specific trade:
//! - Fill prices vs ideal prices
//! - Slippage breakdown
//! - Gap fills
//! - Intrabar ambiguity resolution

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::theme::Theme;

/// Diagnostic data for a single trade
#[derive(Debug, Clone)]
pub struct DiagnosticData {
    pub trade_id: String,
    pub symbol: String,

    // Entry
    pub entry_bar: usize,
    pub entry_ideal_price: f64,
    pub entry_fill_price: f64,
    pub entry_slippage: f64,
    pub entry_gap_fill: bool,

    // Exit
    pub exit_bar: usize,
    pub exit_ideal_price: f64,
    pub exit_fill_price: f64,
    pub exit_slippage: f64,
    pub exit_gap_fill: bool,

    // Ambiguity
    pub ambiguity_note: Option<String>,

    // Signal trace (signal → intent → order → fill)
    pub signal_intent: Option<String>,
    pub order_type: Option<String>,
    pub fill_context: Option<String>,
}

/// Diagnostics widget
pub struct Diagnostics<'a> {
    data: &'a DiagnosticData,
    theme: &'a Theme,
}

impl<'a> Diagnostics<'a> {
    pub fn new(data: &'a DiagnosticData, theme: &'a Theme) -> Self {
        Self { data, theme }
    }

    fn format_slippage(&self, slippage: f64) -> (String, ratatui::style::Color) {
        let color = if slippage.abs() < 0.01 {
            self.theme.positive
        } else if slippage.abs() < 0.50 {
            self.theme.warning
        } else {
            self.theme.negative
        };
        (format!("${:.2}", slippage.abs()), color)
    }

    fn format_price(&self, price: f64) -> String {
        format!("${:.2}", price)
    }
}

impl<'a> Widget for Diagnostics<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" Trade Diagnostics: {} ", self.data.symbol))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(area);
        block.render(area, buf);

        // Build diagnostic lines
        let (entry_slip_str, entry_slip_color) = self.format_slippage(self.data.entry_slippage);
        let (exit_slip_str, exit_slip_color) = self.format_slippage(self.data.exit_slippage);

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("ENTRY", Style::default().fg(self.theme.positive).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Bar:        ", Style::default().fg(self.theme.text_secondary)),
                Span::raw(format!("{}", self.data.entry_bar)),
            ]),
            Line::from(vec![
                Span::styled("  Ideal Price:", Style::default().fg(self.theme.text_secondary)),
                Span::raw(format!(" {}", self.format_price(self.data.entry_ideal_price))),
            ]),
            Line::from(vec![
                Span::styled("  Fill Price: ", Style::default().fg(self.theme.text_secondary)),
                Span::raw(format!(" {}", self.format_price(self.data.entry_fill_price))),
            ]),
            Line::from(vec![
                Span::styled("  Slippage:   ", Style::default().fg(self.theme.text_secondary)),
                Span::styled(entry_slip_str, Style::default().fg(entry_slip_color)),
            ]),
            Line::from(vec![
                Span::styled("  Gap Fill:   ", Style::default().fg(self.theme.text_secondary)),
                Span::styled(
                    if self.data.entry_gap_fill { "Yes" } else { "No" },
                    Style::default().fg(
                        if self.data.entry_gap_fill { self.theme.warning } else { self.theme.positive }
                    ),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("EXIT", Style::default().fg(self.theme.negative).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Bar:        ", Style::default().fg(self.theme.text_secondary)),
                Span::raw(format!("{}", self.data.exit_bar)),
            ]),
            Line::from(vec![
                Span::styled("  Ideal Price:", Style::default().fg(self.theme.text_secondary)),
                Span::raw(format!(" {}", self.format_price(self.data.exit_ideal_price))),
            ]),
            Line::from(vec![
                Span::styled("  Fill Price: ", Style::default().fg(self.theme.text_secondary)),
                Span::raw(format!(" {}", self.format_price(self.data.exit_fill_price))),
            ]),
            Line::from(vec![
                Span::styled("  Slippage:   ", Style::default().fg(self.theme.text_secondary)),
                Span::styled(exit_slip_str, Style::default().fg(exit_slip_color)),
            ]),
            Line::from(vec![
                Span::styled("  Gap Fill:   ", Style::default().fg(self.theme.text_secondary)),
                Span::styled(
                    if self.data.exit_gap_fill { "Yes" } else { "No" },
                    Style::default().fg(
                        if self.data.exit_gap_fill { self.theme.warning } else { self.theme.positive }
                    ),
                ),
            ]),
        ];

        let mut all_lines = lines;

        if let Some(note) = &self.data.ambiguity_note {
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(vec![
                Span::styled("AMBIGUITY", Style::default().fg(self.theme.warning).add_modifier(Modifier::BOLD)),
            ]));
            all_lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(note, Style::default().fg(self.theme.warning)),
            ]));
        }

        // Signal trace section
        if self.data.signal_intent.is_some()
            || self.data.order_type.is_some()
            || self.data.fill_context.is_some()
        {
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(vec![
                Span::styled("SIGNAL TRACE", Style::default().fg(self.theme.neutral).add_modifier(Modifier::BOLD)),
            ]));
            if let Some(signal) = &self.data.signal_intent {
                all_lines.push(Line::from(vec![
                    Span::styled("  Signal:  ", Style::default().fg(self.theme.text_secondary)),
                    Span::raw(signal.to_string()),
                ]));
            }
            if let Some(order) = &self.data.order_type {
                all_lines.push(Line::from(vec![
                    Span::styled("  Order:   ", Style::default().fg(self.theme.text_secondary)),
                    Span::raw(order.to_string()),
                ]));
            }
            if let Some(fill) = &self.data.fill_context {
                all_lines.push(Line::from(vec![
                    Span::styled("  Fill:    ", Style::default().fg(self.theme.text_secondary)),
                    Span::raw(fill.to_string()),
                ]));
            }
        }

        all_lines.push(Line::from(""));
        all_lines.push(Line::from(vec![
            Span::styled(
                "Press Esc to go back",
                Style::default().fg(self.theme.muted).add_modifier(Modifier::ITALIC),
            ),
        ]));

        let paragraph = Paragraph::new(all_lines)
            .alignment(Alignment::Left);

        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_data_creation() {
        let data = DiagnosticData {
            trade_id: "trade_1".to_string(),
            symbol: "AAPL".to_string(),
            entry_bar: 45,
            entry_ideal_price: 105.0,
            entry_fill_price: 105.23,
            entry_slippage: 0.23,
            entry_gap_fill: false,
            exit_bar: 67,
            exit_ideal_price: 118.0,
            exit_fill_price: 118.50,
            exit_slippage: 0.50,
            exit_gap_fill: false,
            ambiguity_note: Some("Stop hit first (WorstCase)".to_string()),
            signal_intent: Some("Long".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $105.23".to_string()),
        };

        assert_eq!(data.trade_id, "trade_1");
        assert_eq!(data.entry_bar, 45);
        assert_eq!(data.exit_bar, 67);
    }

    #[test]
    fn test_diagnostics_format_slippage() {
        let theme = Theme::default();
        let data = DiagnosticData {
            trade_id: "trade_1".to_string(),
            symbol: "AAPL".to_string(),
            entry_bar: 45,
            entry_ideal_price: 105.0,
            entry_fill_price: 105.23,
            entry_slippage: 0.23,
            entry_gap_fill: false,
            exit_bar: 67,
            exit_ideal_price: 118.0,
            exit_fill_price: 118.50,
            exit_slippage: 0.50,
            exit_gap_fill: false,
            ambiguity_note: None,
            signal_intent: None,
            order_type: None,
            fill_context: None,
        };

        let diag = Diagnostics::new(&data, &theme);
        let (slip_str, _) = diag.format_slippage(0.23);
        assert_eq!(slip_str, "$0.23");
    }
}
