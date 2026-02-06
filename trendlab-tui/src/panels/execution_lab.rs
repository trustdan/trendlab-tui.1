//! Execution lab panel - execution sensitivity analysis
//!
//! Allows rerunning with different execution presets:
//! - Deterministic (baseline)
//! - WorstCase (conservative)
//! - PathMC (Monte Carlo)
//! - Different slippage/spread assumptions

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::theme::Theme;

/// State of a preset rerun
#[derive(Debug, Clone)]
pub enum PresetState {
    /// Not yet run
    NotRun,
    /// Currently running in background
    Running,
    /// Completed successfully
    Complete,
    /// Failed with error
    Failed(String),
}

/// Execution preset option
#[derive(Debug, Clone)]
pub struct ExecutionPreset {
    pub name: String,
    pub description: String,
    pub sharpe: Option<f64>,     // Result after rerun (if available)
    pub total_return: Option<f64>,
    pub state: PresetState,
}

/// Execution lab panel widget
pub struct ExecutionLabPanel<'a> {
    presets: &'a [ExecutionPreset],
    selected_index: usize,
    theme: &'a Theme,
}

impl<'a> ExecutionLabPanel<'a> {
    pub fn new(presets: &'a [ExecutionPreset], selected_index: usize, theme: &'a Theme) -> Self {
        Self {
            presets,
            selected_index,
            theme,
        }
    }
}

impl<'a> Widget for ExecutionLabPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Execution Lab - Sensitivity Analysis ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.neutral))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "Rerun strategy with different execution assumptions:",
                Style::default()
                    .fg(self.theme.text_secondary)
                    .add_modifier(Modifier::ITALIC),
            )]),
            Line::from(""),
        ];

        for (i, preset) in self.presets.iter().enumerate() {
            let is_selected = i == self.selected_index;

            let marker = if is_selected { "▶" } else { " " };
            let style = if is_selected {
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(self.theme.accent)),
                Span::styled(format!(" {}", preset.name), style),
            ]));

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("  {}", preset.description),
                    Style::default().fg(self.theme.muted),
                ),
            ]));

            match &preset.state {
                PresetState::Complete => {
                    if let (Some(sharpe), Some(ret)) = (preset.sharpe, preset.total_return) {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("  Sharpe: {:.2} | Return: {:+.2}%", sharpe, ret * 100.0),
                                Style::default()
                                    .fg(self.theme.sharpe_color(sharpe))
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                }
                PresetState::Running => {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            "  [Running...]",
                            Style::default()
                                .fg(self.theme.accent)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }
                PresetState::Failed(err) => {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("  [FAILED: {}]", err),
                            Style::default()
                                .fg(self.theme.negative)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
                PresetState::NotRun => {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            "  [Not yet run - press Enter]",
                            Style::default()
                                .fg(self.theme.muted)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }
            }

            lines.push(Line::from(""));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Press Enter to rerun with selected preset | Esc to go back",
            Style::default()
                .fg(self.theme.muted)
                .add_modifier(Modifier::ITALIC),
        )]));

        let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_lab_panel_creation() {
        let theme = Theme::default();
        let presets = vec![
            ExecutionPreset {
                name: "Deterministic".to_string(),
                description: "Fixed slippage, no ambiguity".to_string(),
                sharpe: Some(2.5),
                total_return: Some(0.45),
                state: PresetState::Complete,
            },
            ExecutionPreset {
                name: "WorstCase".to_string(),
                description: "Conservative ambiguity resolution".to_string(),
                sharpe: None,
                total_return: None,
                state: PresetState::NotRun,
            },
        ];

        let panel = ExecutionLabPanel::new(&presets, 0, &theme);
        assert_eq!(panel.selected_index, 0);
    }

    #[test]
    fn test_execution_lab_renders_all_states() {
        let theme = Theme::default();
        let presets = vec![
            ExecutionPreset {
                name: "Deterministic".to_string(),
                description: "Baseline".to_string(),
                sharpe: Some(2.5),
                total_return: Some(0.45),
                state: PresetState::Complete,
            },
            ExecutionPreset {
                name: "WorstCase".to_string(),
                description: "Conservative".to_string(),
                sharpe: None,
                total_return: None,
                state: PresetState::Running,
            },
            ExecutionPreset {
                name: "BestCase".to_string(),
                description: "Optimistic".to_string(),
                sharpe: None,
                total_return: None,
                state: PresetState::NotRun,
            },
            ExecutionPreset {
                name: "PathMC".to_string(),
                description: "Monte Carlo".to_string(),
                sharpe: None,
                total_return: None,
                state: PresetState::Failed("timeout".to_string()),
            },
        ];

        let panel = ExecutionLabPanel::new(&presets, 0, &theme);
        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Verify it renders without panic
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("Running"));
        assert!(content.contains("FAILED"));
        assert!(content.contains("Not yet run"));
    }
}
