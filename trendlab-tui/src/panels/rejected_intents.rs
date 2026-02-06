//! Rejected intents panel - shows blocked signals timeline
//!
//! Critical for debugging "why did the strategy stop trading?"
//!
//! Displays:
//! - Timeline of rejected signals
//! - Rejection reasons (VolatilityGuard, LiquidityGuard, MarginGuard, RiskGuard)
//! - Rejection rate per reason type
//! - Context values (e.g., volatility exceeded threshold)

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Widget},
};
use crate::theme::Theme;

/// Rejected intent record
#[derive(Debug, Clone)]
pub struct RejectedIntentRecord {
    pub bar_index: usize,
    pub date: String,
    pub signal: String, // "Long", "Short", "Flat"
    pub rejection_reason: String,
    pub context: String, // e.g., "volatility=0.05, threshold=0.03"
}

/// Rejection statistics
#[derive(Debug, Clone)]
pub struct RejectionStats {
    pub total_signals: usize,
    pub total_rejected: usize,
    pub by_reason: Vec<(String, usize)>, // (reason, count)
}

impl RejectionStats {
    pub fn rejection_rate(&self) -> f64 {
        if self.total_signals == 0 {
            0.0
        } else {
            (self.total_rejected as f64) / (self.total_signals as f64)
        }
    }
}

/// Rejected intents panel widget
pub struct RejectedIntentsPanel<'a> {
    records: &'a [RejectedIntentRecord],
    stats: &'a RejectionStats,
    theme: &'a Theme,
}

impl<'a> RejectedIntentsPanel<'a> {
    pub fn new(
        records: &'a [RejectedIntentRecord],
        stats: &'a RejectionStats,
        theme: &'a Theme,
    ) -> Self {
        Self {
            records,
            stats,
            theme,
        }
    }
}

impl<'a> Widget for RejectedIntentsPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rejection_rate = self.stats.rejection_rate() * 100.0;

        let title = format!(
            " Rejected Intents │ {:.1}% rejected ({}/{}) ",
            rejection_rate, self.stats.total_rejected, self.stats.total_signals
        );

        // Split area: top 30% for stats, bottom 70% for timeline
        let stats_height = (area.height as f32 * 0.3) as u16;
        let timeline_height = area.height.saturating_sub(stats_height);

        let stats_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: stats_height,
        };

        let timeline_area = Rect {
            x: area.x,
            y: area.y + stats_height,
            width: area.width,
            height: timeline_height,
        };

        // Render stats section
        let stats_block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.warning))
            .style(Style::default().bg(self.theme.background));

        let stats_inner = stats_block.inner(stats_area);
        stats_block.render(stats_area, buf);

        let mut stats_lines = vec![Line::from("")];
        for (reason, count) in &self.stats.by_reason {
            let percentage = if self.stats.total_rejected > 0 {
                (*count as f64 / self.stats.total_rejected as f64) * 100.0
            } else {
                0.0
            };

            stats_lines.push(Line::from(vec![
                Span::styled(
                    format!("{:20}", reason),
                    Style::default().fg(self.theme.rejection_color(reason)),
                ),
                Span::styled(
                    format!("{:>5} ({:>5.1}%)", count, percentage),
                    Style::default()
                        .fg(self.theme.text_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        let stats_paragraph = Paragraph::new(stats_lines).alignment(Alignment::Left);
        stats_paragraph.render(stats_inner, buf);

        // Render timeline table
        let timeline_block = Block::default()
            .title(" Timeline ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.warning))
            .style(Style::default().bg(self.theme.background));

        let header_cells = ["Bar", "Date", "Signal", "Reason", "Context"]
            .iter()
            .map(|h| {
                Cell::from(*h).style(
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
            });

        let header = Row::new(header_cells).height(1);

        let rows = self.records.iter().map(|record| {
            let reason_color = self.theme.rejection_color(&record.rejection_reason);

            let cells = vec![
                Cell::from(format!("{}", record.bar_index)),
                Cell::from(record.date.clone()),
                Cell::from(record.signal.clone())
                    .style(Style::default().fg(self.theme.signal_color(&record.signal))),
                Cell::from(record.rejection_reason.clone()).style(Style::default().fg(reason_color)),
                Cell::from(record.context.clone()).style(Style::default().fg(self.theme.muted)),
            ];

            Row::new(cells).height(1)
        });

        let widths = [
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(18),
            Constraint::Percentage(40),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(timeline_block)
            .column_spacing(1);

        table.render(timeline_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejection_stats_rate() {
        let stats = RejectionStats {
            total_signals: 100,
            total_rejected: 87,
            by_reason: vec![("VolatilityGuard".to_string(), 87)],
        };

        assert_eq!(stats.rejection_rate(), 0.87);
    }

    #[test]
    fn test_rejected_intents_panel_creation() {
        let theme = Theme::default();
        let records = vec![RejectedIntentRecord {
            bar_index: 45,
            date: "2023-01-15".to_string(),
            signal: "Long".to_string(),
            rejection_reason: "VolatilityGuard".to_string(),
            context: "volatility=0.05, threshold=0.03".to_string(),
        }];

        let stats = RejectionStats {
            total_signals: 100,
            total_rejected: 87,
            by_reason: vec![("VolatilityGuard".to_string(), 87)],
        };

        let panel = RejectedIntentsPanel::new(&records, &stats, &theme);
        assert_eq!(panel.records.len(), 1);
    }
}
