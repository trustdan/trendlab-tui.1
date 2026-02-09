//! Panel 2 — Strategy: four-component composition selection with parameter sliders.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::AppState;
use crate::theme;

const COMPONENT_LABELS: [&str; 4] = ["Signal", "Position Manager", "Execution", "Signal Filter"];

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let s = &app.strategy;
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("[j/k]navigate [h/l]adjust [Enter]run backtest", theme::muted()),
    ]));
    lines.push(Line::from(""));

    let component_data: Vec<(usize, &str, &str, &[f64])> = vec![
        (0, COMPONENT_LABELS[0], s.pool.signals[s.signal_idx].component_type.as_str(), &s.signal_params),
        (1, COMPONENT_LABELS[1], s.pool.position_managers[s.pm_idx].component_type.as_str(), &s.pm_params),
        (2, COMPONENT_LABELS[2], s.pool.execution_models[s.exec_idx].component_type.as_str(), &s.exec_params),
        (3, COMPONENT_LABELS[3], s.pool.filters[s.filter_idx].component_type.as_str(), &s.filter_params),
    ];

    let variants_list: Vec<&[trendlab_core::components::sampler::ComponentVariant]> = vec![
        &s.pool.signals,
        &s.pool.position_managers,
        &s.pool.execution_models,
        &s.pool.filters,
    ];
    let idx_list = [s.signal_idx, s.pm_idx, s.exec_idx, s.filter_idx];

    for (comp_idx, label, type_name, params) in &component_data {
        let is_active_comp = *comp_idx == s.active_component;

        // Component header
        let header_style = if is_active_comp && s.active_param == 0
            && variants_list[*comp_idx][idx_list[*comp_idx]].param_ranges.is_empty()
        {
            theme::accent().add_modifier(Modifier::REVERSED)
        } else if is_active_comp {
            theme::accent_bold()
        } else {
            theme::neutral()
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), header_style),
            Span::styled(
                format!("< {type_name} >"),
                if is_active_comp { theme::accent() } else { theme::muted() },
            ),
        ]));

        // Parameters
        let variant = &variants_list[*comp_idx][idx_list[*comp_idx]];
        for (pi, range) in variant.param_ranges.iter().enumerate() {
            let val = if pi < params.len() { params[pi] } else { range.default };
            let is_active_param = is_active_comp && pi == s.active_param;

            let bar = render_slider_inline(val, range.min, range.max, 20);

            let param_style = if is_active_param {
                theme::accent().add_modifier(Modifier::REVERSED)
            } else {
                theme::muted()
            };

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:>18}: ", range.name), param_style),
                Span::styled(bar, if is_active_param { theme::accent() } else { theme::muted() }),
                Span::styled(format!(" {val:.2}"), param_style),
            ]));
        }

        lines.push(Line::from(""));
    }

    // Trading mode
    lines.push(Line::from(vec![
        Span::styled("Mode: ", theme::muted()),
        Span::styled(format!("{:?}", s.trading_mode), theme::accent()),
        Span::styled(
            format!("  Capital: ${:.0}  Size: {:.0}%", s.initial_capital, s.position_size_pct),
            theme::muted(),
        ),
    ]));

    let para = Paragraph::new(lines);
    f.render_widget(para, area);
}

fn render_slider_inline(value: f64, min: f64, max: f64, width: usize) -> String {
    let range = max - min;
    if range <= 0.0 {
        return format!("[{}]", "=".repeat(width));
    }
    let frac = ((value - min) / range).clamp(0.0, 1.0);
    let filled = (frac * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "=".repeat(filled), " ".repeat(empty))
}
