//! Panel 3 — Sweep: YOLO mode configuration, dual sliders, launch/stop.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::AppState;
use crate::theme;

const SETTING_LABELS: [&str; 12] = [
    "Parameter Jitter",
    "Structural Explore",
    "Start Date",
    "End Date",
    "Initial Capital",
    "Fitness Metric",
    "Sweep Depth",
    "Warmup Iterations",
    "Polars Threads",
    "Outer Threads",
    "Max Iterations",
    "Master Seed",
];

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let s = &app.sweep;
    let c = &s.config;
    let mut lines: Vec<Line> = Vec::new();

    if s.yolo_running {
        lines.push(Line::from(vec![
            Span::styled("YOLO RUNNING ", theme::positive()),
            Span::styled("[Esc]stop", theme::muted()),
        ]));

        if let Some(p) = &s.last_progress {
            lines.push(Line::from(vec![
                Span::styled(format!("Iteration: {} ", p.iteration), theme::accent()),
                Span::styled(format!("| {} ", p.current_symbol), theme::neutral()),
                Span::styled(
                    format!(
                        "| {:.0} iter/min | {} ok / {} err",
                        p.throughput_per_min, p.success_count, p.error_count
                    ),
                    theme::muted(),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        "Leaderboard: {} entries | Cross: {} | L2: {} L3: {}",
                        p.leaderboard_entries, p.cross_leaderboard_entries,
                        p.promoted_l2_count, p.promoted_l3_count,
                    ),
                    theme::muted(),
                ),
            ]));
        }

        lines.push(Line::from(""));
    } else {
        lines.push(Line::from(vec![
            Span::styled("[j/k]navigate [h/l]adjust [Enter]start YOLO", theme::muted()),
        ]));
        lines.push(Line::from(""));
    }

    // Settings
    let values: Vec<String> = vec![
        format!("{:.0}%", c.jitter_pct * 100.0),
        format!("{:.0}%", c.structural_explore * 100.0),
        c.start_date.to_string(),
        c.end_date.to_string(),
        format!("${:.0}", c.initial_capital),
        format!("{:?}", c.fitness_metric),
        format!("{:?}", c.sweep_depth),
        c.warmup_iterations.to_string(),
        c.polars_thread_cap.to_string(),
        c.outer_thread_cap.to_string(),
        c.max_iterations
            .map(|n| n.to_string())
            .unwrap_or_else(|| "unlimited".into()),
        c.master_seed.to_string(),
    ];

    for (i, (label, value)) in SETTING_LABELS.iter().zip(values.iter()).enumerate() {
        let is_active = i == s.cursor && !s.yolo_running;

        let style = if is_active {
            theme::accent().add_modifier(Modifier::REVERSED)
        } else {
            theme::muted()
        };

        // Render slider for jitter and structural explore
        if i < 2 {
            let frac = if i == 0 { c.jitter_pct } else { c.structural_explore };
            let bar_width: usize = 30;
            let filled = (frac * bar_width as f64).round() as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar = format!("[{}{}]", "=".repeat(filled), " ".repeat(empty));

            lines.push(Line::from(vec![
                Span::styled(format!("{:>20}: ", label), style),
                Span::styled(bar, if is_active { theme::accent() } else { theme::muted() }),
                Span::styled(format!(" {value}"), style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("{:>20}: ", label), style),
                Span::styled(value.as_str(), style),
            ]));
        }
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, area);
}
