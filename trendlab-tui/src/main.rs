//! TrendLab TUI — six-panel terminal interface with vim-style navigation.
//!
//! Panels:
//! 1. Data — sector/ticker hierarchy, data fetching, cache status
//! 2. Strategy — four-component composition selection
//! 3. Sweep — YOLO mode configuration and launch
//! 4. Results — leaderboard display with rankings
//! 5. Chart — equity curve visualization
//! 6. Help — keyboard shortcuts and documentation

mod app;
mod input;
mod persistence;
mod theme;
mod ui;
mod worker;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use trendlab_core::data::cache::ParquetCache;

use crate::app::{AppState, ErrorCategory};
use crate::worker::{WorkerCommand, WorkerResponse};

fn main() -> Result<()> {
    // Install a panic hook that restores the terminal before printing the panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
        default_hook(info);
    }));

    // Paths
    let cache_dir = PathBuf::from("data");
    let state_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("trendlab")
        .join("state.json");

    // Load persisted state
    let persisted = persistence::load(&state_path);

    // Worker channels
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));

    // Spawn worker
    let worker_handle = worker::spawn_worker(cmd_rx, resp_tx, cancel.clone());

    // Build app state
    let mut app = AppState::new(
        cmd_tx.clone(),
        resp_rx,
        cancel.clone(),
        cache_dir.clone(),
        state_path.clone(),
    );

    // Apply persisted state
    persistence::apply(&mut app, persisted);

    // Scan cache for existing data
    scan_cache_status(&mut app, &cache_dir);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Run the main event loop
    let result = run_app(&mut terminal, &mut app);

    // Save state before exit
    let persisted = persistence::extract(&app);
    let _ = persistence::save(&state_path, &persisted);

    // Shutdown worker
    let _ = cmd_tx.send(WorkerCommand::Shutdown);
    let _ = worker_handle.join();

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> Result<()> {
    loop {
        // 1. Render
        terminal.draw(|f| ui::draw(f, app))?;

        // 2. Drain worker responses (non-blocking)
        while let Ok(resp) = app.worker_rx.try_recv() {
            handle_worker_response(app, resp);
        }

        // 3. Poll for input events (50ms timeout for ~20 FPS tick)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                input::handle_key(app, key);
            }
        }

        // 4. Check quit
        if !app.running {
            break;
        }
    }
    Ok(())
}

fn handle_worker_response(app: &mut AppState, resp: WorkerResponse) {
    match resp {
        WorkerResponse::FetchProgress {
            symbol,
            index,
            total,
        } => {
            app.data.fetch_current_symbol = Some(symbol);
            app.data.fetch_done = index;
            app.data.fetch_total = total;
        }
        WorkerResponse::FetchSymbolDone {
            symbol,
            success,
            error,
        } => {
            if success {
                app.data.cache_status.insert(symbol, true);
            } else if let Some(err) = error {
                app.push_error(
                    ErrorCategory::Network,
                    format!("Failed to fetch: {err}"),
                    symbol,
                );
            }
            app.data.fetch_done += 1;
        }
        WorkerResponse::FetchBatchDone { succeeded, failed } => {
            app.data.fetch_in_progress = false;
            app.data.fetch_current_symbol = None;
            if failed == 0 {
                app.set_status(format!("Fetch complete: {succeeded} symbols downloaded"));
            } else {
                app.set_warning(format!("Fetch done: {succeeded} ok, {failed} failed"));
            }
        }
        WorkerResponse::BacktestComplete { result } => {
            let entry = app::LeaderboardDisplayEntry {
                rank: app.results.entries.len() + 1,
                signal_type: result.config.signal.component_type.clone(),
                pm_type: result.config.position_manager.component_type.clone(),
                exec_type: result.config.execution_model.component_type.clone(),
                filter_type: result.config.signal_filter.component_type.clone(),
                symbol: result.symbol.clone(),
                sharpe: result.metrics.sharpe,
                cagr: result.metrics.cagr,
                max_drawdown: result.metrics.max_drawdown,
                win_rate: result.metrics.win_rate,
                profit_factor: result.metrics.profit_factor,
                trade_count: result.metrics.trade_count,
                fitness_score: result.metrics.sharpe,
                session_id: app.results.current_session_id.clone(),
                config: result.config.clone(),
                metrics: result.metrics.clone(),
                stickiness: result.stickiness.clone(),
            };

            // Populate chart with equity curve
            app.chart.equity_curve = Some(result.equity_curve.clone());
            app.chart.label = format!(
                "{} | {} | Sharpe: {:.2}",
                entry.symbol, entry.signal_type, entry.sharpe
            );

            app.results.entries.push(entry);
            app.set_status(format!(
                "Backtest complete: {} trades, Sharpe {:.2}",
                result.metrics.trade_count, result.metrics.sharpe
            ));
        }
        WorkerResponse::BacktestError { error } => {
            app.push_error(ErrorCategory::Engine, error, "single backtest".into());
        }
        WorkerResponse::YoloProgress(progress) => {
            app.sweep.last_progress = Some(progress);
        }
        WorkerResponse::YoloDone { result } => {
            app.sweep.yolo_running = false;
            app.sweep.last_progress = None;
            app.set_status(format!(
                "YOLO complete: {} iterations, {} ok, {} errors in {:.1}s",
                result.iterations_completed,
                result.success_count,
                result.error_count,
                result.elapsed_secs,
            ));
        }
        WorkerResponse::YoloError { error } => {
            app.sweep.yolo_running = false;
            app.sweep.last_progress = None;
            app.push_error(ErrorCategory::Engine, error, "YOLO mode".into());
        }
        WorkerResponse::EquityCurve {
            index: _,
            curve,
            label,
        } => {
            app.chart.equity_curve = Some(curve);
            app.chart.label = label;
        }
        WorkerResponse::Error {
            category,
            message,
            context,
        } => {
            let cat = match category.as_str() {
                "network" => ErrorCategory::Network,
                "data" => ErrorCategory::Data,
                "engine" => ErrorCategory::Engine,
                "nan" => ErrorCategory::NanMetrics,
                _ => ErrorCategory::Other,
            };
            app.push_error(cat, message, context);
        }
    }
}

fn scan_cache_status(app: &mut AppState, cache_dir: &PathBuf) {
    let cache = ParquetCache::new(cache_dir);
    let all_tickers: Vec<&str> = app.data.universe.all_tickers();
    let statuses = cache.status(&all_tickers);
    for status in statuses {
        app.data
            .cache_status
            .insert(status.symbol.clone(), status.cached);
    }
}
