//! TrendLab v3 TUI - Main entry point
//!
//! Launches the terminal UI for exploring backtesting results.

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
    Terminal,
};
use std::{env, io, path::PathBuf, sync::Arc};
use trendlab_tui::{
    app::App,
    backtest_service::RunnerService,
    drill_down::{DrillDownState, SummaryCard, Diagnostics},
    data_loader::{LoadConfig, load_results},
    navigation::handle_key_event,
    app::ChartMode,
    panels::{
        candle_chart::{ohlc_from_equity, overlays_from_trades, CandleChartPanel},
        ChartPanel, ExecutionLabPanel, LeaderboardPanel, RejectedIntentsPanel,
        RobustnessPanel, RunManifestPanel, SensitivityPanel, TradeTapePanel,
    },
    sample_data::sample_results,
};

fn main() -> Result<()> {
    let load_config = parse_args()?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app with backtest service
    let mut app = App::new();
    let service = Arc::new(RunnerService::new());
    app.set_backtest_service(service);

    let mut results = load_results(&load_config)?;
    if results.is_empty() {
        app.set_error("No results found; showing sample data".to_string());
        results = sample_results();
    }
    app.load_results(results);

    // Run main loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn parse_args() -> Result<LoadConfig> {
    let mut config = LoadConfig::empty();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--results" | "--results-path" => {
                if let Some(path) = args.next() {
                    config.results_path = Some(PathBuf::from(path));
                }
            }
            "--cache-dir" => {
                if let Some(path) = args.next() {
                    config.cache_dir = Some(PathBuf::from(path));
                }
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
    }

    Ok(config)
}

fn print_help() {
    println!("TrendLab TUI");
    println!();
    println!("Usage:");
    println!("  trendlab-tui [--results <file|dir>] [--cache-dir <dir>]");
    println!();
    println!("Options:");
    println!("  --results, --results-path   Load results from a file or directory");
    println!("  --cache-dir                 Load results from a cache directory");
    println!("  -h, --help                  Show this help message");
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        // Poll for completed reruns before rendering
        app.poll_reruns();

        // Draw UI
        terminal.draw(|f| {
            let area = f.area();
            render_ui(f, app, area);
        })?;

        // Handle events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Clear any notice on first keypress
                app.clear_error();
                handle_key_event(app, key);
            }
        }

        // Check if should quit
        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn render_ui(f: &mut ratatui::Frame, app: &App, area: Rect) {
    // Render based on current drill-down state
    match &app.drill_down {
        DrillDownState::Leaderboard => {
            render_leaderboard(f, app, area);
        }

        DrillDownState::SummaryCard(run_id) => {
            // Render leaderboard as background
            render_leaderboard(f, app, area);
            if let Some(summary) = app.summary_card_data(run_id) {
                let overlay = SummaryCard::new(&summary, &app.theme);
                f.render_widget(overlay, area);
            }
        }

        DrillDownState::TradeTape(run_id) => {
            let trades = app.trade_records(run_id);
            let panel = TradeTapePanel::new(&trades, app.selected_index, &app.theme);
            f.render_widget(panel, area);
        }

        DrillDownState::ChartWithTrade(run_id, _trade_id) => {
            match app.chart_mode {
                ChartMode::EquityCurve => {
                    if let Some(ghost_curve) = app.ghost_curve(run_id) {
                        let markers = app.trade_markers(run_id);
                        let panel = ChartPanel::new(&ghost_curve, &markers, &app.theme);
                        f.render_widget(panel, area);
                    } else {
                        render_leaderboard(f, app, area);
                    }
                }
                ChartMode::CandleChart => {
                    if let Some(result) = app.results_by_id.get(run_id) {
                        let equity_values: Vec<f64> =
                            result.equity_curve.iter().map(|p| p.equity).collect();
                        let bars = ohlc_from_equity(&equity_values);
                        let overlays = overlays_from_trades(&result.trades);
                        let symbol = result
                            .trades
                            .first()
                            .map(|t| t.symbol.as_str())
                            .unwrap_or("???");
                        let panel =
                            CandleChartPanel::new(&bars, &overlays, symbol, &app.theme);
                        f.render_widget(panel, area);
                    } else {
                        render_leaderboard(f, app, area);
                    }
                }
            }
        }

        DrillDownState::Diagnostics(run_id, trade_id) => {
            if let Some(data) = app.diagnostics_for_trade(run_id, trade_id) {
                let panel = Diagnostics::new(&data, &app.theme);
                f.render_widget(panel, area);
            } else {
                render_leaderboard(f, app, area);
            }
        }

        DrillDownState::RejectedIntents(run_id) => {
            let (records, stats) = app.rejected_intents(run_id);
            let panel = RejectedIntentsPanel::new(&records, &stats, &app.theme);
            f.render_widget(panel, area);
        }

        DrillDownState::ExecutionLab(run_id) => {
            let presets = app.execution_presets(run_id);
            let panel = ExecutionLabPanel::new(&presets, app.selected_index, &app.theme);
            f.render_widget(panel, area);
        }

        DrillDownState::Sensitivity(run_id) => {
            if let Some(base_result) = app.results_by_id.get(run_id) {
                let reruns = app.completed_reruns(run_id);
                let panel = SensitivityPanel::new(base_result, &reruns, &app.theme);
                f.render_widget(panel, area);
            } else {
                render_leaderboard(f, app, area);
            }
        }

        DrillDownState::RunManifest(run_id) => {
            if let Some(result) = app.results_by_id.get(run_id) {
                if let Some(config) = &result.metadata.config {
                    let panel = RunManifestPanel::new(config, run_id, &app.theme);
                    f.render_widget(panel, area);
                } else {
                    render_leaderboard(f, app, area);
                }
            } else {
                render_leaderboard(f, app, area);
            }
        }

        DrillDownState::Robustness(run_id) => {
            // For now, show sample robustness data from runner if available
            // In a full integration, this would come from a background robustness run
            let levels = app.robustness_levels(run_id);
            let panel = RobustnessPanel::new(&levels, 0, &app.theme);
            f.render_widget(panel, area);
        }
    }

    if let Some(message) = &app.error_message {
        render_message_overlay(f, area, message, &app.theme);
    }
}

fn render_leaderboard(f: &mut ratatui::Frame, app: &App, area: Rect) {
    if app.results.is_empty() {
        render_empty_state(f, area, &app.theme);
        return;
    }

    let sorted = app.sorted_results();
    let panel = LeaderboardPanel::new(
        &sorted,
        app.selected_index,
        &app.theme,
        app.fitness_metric,
        app.show_session_only,
    );
    f.render_widget(panel, area);
}

fn render_empty_state(f: &mut ratatui::Frame, area: Rect, theme: &trendlab_tui::Theme) {
    let block = Block::default()
        .title(" No Results Loaded ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.background));

    let inner = block.inner(area);
    block.render(area, f.buffer_mut());

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Use ", Style::default().fg(theme.text_secondary)),
            Span::styled("--results", Style::default().fg(theme.accent)),
            Span::styled(" or ", Style::default().fg(theme.text_secondary)),
            Span::styled("--cache-dir", Style::default().fg(theme.accent)),
            Span::styled(" to load data.", Style::default().fg(theme.text_secondary)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Sample data will appear when no files are found.",
            Style::default().fg(theme.muted),
        )]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(Block::default())
        .alignment(Alignment::Center);
    f.render_widget(paragraph, inner);
}

fn render_message_overlay(
    f: &mut ratatui::Frame,
    area: Rect,
    message: &str,
    theme: &trendlab_tui::Theme,
) {
    let overlay_height = 3;
    if area.height <= overlay_height {
        return;
    }

    let overlay_area = Rect {
        x: area.x,
        y: area.y + area.height - overlay_height,
        width: area.width,
        height: overlay_height,
    };

    // Clear the area first so no underlying text bleeds through
    ratatui::widgets::Clear.render(overlay_area, f.buffer_mut());

    let block = Block::default()
        .title(" Notice ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.background));

    let inner = block.inner(overlay_area);
    block.render(overlay_area, f.buffer_mut());

    let lines = vec![
        Line::from(vec![Span::styled(
            message,
            Style::default().fg(theme.warning),
        )]),
    ];

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_runs() {
        // Just verify the app can be created
        let app = App::new();
        assert!(!app.should_quit);
    }
}
