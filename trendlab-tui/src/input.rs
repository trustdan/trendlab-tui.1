//! Keyboard input dispatch — global keys → overlays → panel-specific handlers.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use trendlab_runner::RiskProfile;

use crate::app::{
    AppState, Overlay, Panel, SessionFilter, TreeItem,
};

/// Handle a key event. Returns true if the app should continue running.
pub fn handle_key(app: &mut AppState, key: KeyEvent) {
    // Only handle key press events (Windows sends both Press and Release).
    if key.kind != KeyEventKind::Press {
        return;
    }

    // 1. Overlays consume input first.
    match &app.overlay {
        Overlay::Welcome => {
            app.overlay = Overlay::None;
            return;
        }
        Overlay::ErrorHistory => {
            handle_error_overlay(app, key);
            return;
        }
        Overlay::Search => {
            handle_search_overlay(app, key);
            return;
        }
        Overlay::Detail(_) => {
            handle_detail_overlay(app, key);
            return;
        }
        Overlay::None => {}
    }

    // 2. Global keys (always available).
    match key.code {
        KeyCode::Char('q') => {
            app.running = false;
            return;
        }
        KeyCode::Char('1') => { app.active_panel = Panel::Data; return; }
        KeyCode::Char('2') => { app.active_panel = Panel::Strategy; return; }
        KeyCode::Char('3') => { app.active_panel = Panel::Sweep; return; }
        KeyCode::Char('4') => { app.active_panel = Panel::Results; return; }
        KeyCode::Char('5') => { app.active_panel = Panel::Chart; return; }
        KeyCode::Char('6') => { app.active_panel = Panel::Help; return; }
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.active_panel = app.active_panel.prev();
            } else {
                app.active_panel = app.active_panel.next();
            }
            return;
        }
        KeyCode::BackTab => {
            app.active_panel = app.active_panel.prev();
            return;
        }
        _ => {}
    }

    // 3. Panel-specific keys.
    match app.active_panel {
        Panel::Data => handle_data_key(app, key),
        Panel::Strategy => handle_strategy_key(app, key),
        Panel::Sweep => handle_sweep_key(app, key),
        Panel::Results => handle_results_key(app, key),
        Panel::Chart => {} // display only
        Panel::Help => handle_help_key(app, key),
    }
}

fn handle_error_overlay(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('e') => {
            app.overlay = Overlay::None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if app.error_scroll + 1 < app.error_history.len() {
                app.error_scroll += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.error_scroll = app.error_scroll.saturating_sub(1);
        }
        _ => {}
    }
}

fn handle_search_overlay(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::None;
            app.search_input.clear();
        }
        KeyCode::Enter => {
            let symbol = app.search_input.trim().to_uppercase();
            if !symbol.is_empty() {
                // Add to universe under "Custom" sector and select it.
                app.data
                    .universe
                    .sectors
                    .entry("Custom".to_string())
                    .or_default()
                    .push(symbol.clone());
                app.data.selected.insert(symbol.clone());
                app.data.expanded_sectors.insert("Custom".to_string());
                app.set_status(format!("Added {symbol}"));
            }
            app.search_input.clear();
            app.overlay = Overlay::None;
        }
        KeyCode::Backspace => {
            app.search_input.pop();
        }
        KeyCode::Char(c) => {
            app.search_input.push(c);
        }
        _ => {}
    }
}

fn handle_detail_overlay(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
            app.overlay = Overlay::None;
        }
        _ => {}
    }
}

fn handle_data_key(app: &mut AppState, key: KeyEvent) {
    let row_count = app.data.visible_row_count();

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if row_count > 0 && app.data.cursor.row + 1 < row_count {
                app.data.cursor.row += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.data.cursor.row = app.data.cursor.row.saturating_sub(1);
        }
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
            // Expand sector
            if let Some(TreeItem::Sector(name)) = app.data.cursor_item() {
                app.data.expanded_sectors.insert(name);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            // Collapse sector
            if let Some(TreeItem::Sector(name)) = app.data.cursor_item() {
                app.data.expanded_sectors.remove(&name);
            }
        }
        KeyCode::Char(' ') => {
            // Toggle selection
            match app.data.cursor_item() {
                Some(TreeItem::Sector(sector)) => {
                    if let Some(tickers) = app.data.universe.sector_tickers(&sector) {
                        let all_selected = tickers.iter().all(|t| app.data.selected.contains(t));
                        for ticker in tickers {
                            if all_selected {
                                app.data.selected.remove(ticker);
                            } else {
                                app.data.selected.insert(ticker.clone());
                            }
                        }
                    }
                }
                Some(TreeItem::Ticker(_, ticker)) => {
                    if app.data.selected.contains(&ticker) {
                        app.data.selected.remove(&ticker);
                    } else {
                        app.data.selected.insert(ticker);
                    }
                }
                None => {}
            }
        }
        KeyCode::Char('a') => {
            // Select all
            for ticker in app.data.universe.all_tickers() {
                app.data.selected.insert(ticker.to_string());
            }
        }
        KeyCode::Char('d') => {
            // Deselect all
            app.data.selected.clear();
        }
        KeyCode::Char('f') => {
            // Fetch selected tickers
            if !app.data.selected.is_empty() && !app.data.fetch_in_progress {
                let symbols: Vec<String> = app.data.selected.iter().cloned().collect();
                let start = app.sweep.config.start_date;
                let end = app.sweep.config.end_date;
                app.data.fetch_in_progress = true;
                app.data.fetch_done = 0;
                app.data.fetch_total = symbols.len();
                let _ = app.worker_tx.send(crate::worker::WorkerCommand::FetchData {
                    symbols,
                    start,
                    end,
                    cache_dir: app.cache_dir.clone(),
                });
                app.set_status("Fetching data...");
            }
        }
        KeyCode::Char('s') => {
            app.overlay = Overlay::Search;
            app.search_input.clear();
        }
        KeyCode::Esc => {
            if app.data.fetch_in_progress {
                app.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                app.set_warning("Cancelling fetch...");
            }
        }
        _ => {}
    }
}

fn handle_strategy_key(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            // Move to next component or param
            let param_count = app
                .strategy
                .active_variants()[app.strategy.active_idx()]
                .param_ranges
                .len();
            if app.strategy.active_param + 1 < param_count {
                app.strategy.active_param += 1;
            } else if app.strategy.active_component < 3 {
                app.strategy.active_component += 1;
                app.strategy.active_param = 0;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.strategy.active_param > 0 {
                app.strategy.active_param -= 1;
            } else if app.strategy.active_component > 0 {
                app.strategy.active_component -= 1;
                let prev_variant = &app.strategy.active_variants()[app.strategy.active_idx()];
                app.strategy.active_param =
                    prev_variant.param_ranges.len().saturating_sub(1);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            // If on param row: decrease value. If on component header: cycle type left.
            adjust_strategy(app, -1);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            adjust_strategy(app, 1);
        }
        KeyCode::Enter => {
            // Launch single backtest
            if app.data.selected.is_empty() {
                app.set_warning("Select tickers in Data panel first");
                return;
            }
            let config = app.strategy.to_strategy_config();
            let symbols: Vec<String> = app.data.selected.iter().cloned().collect();
            let _ = app.worker_tx.send(crate::worker::WorkerCommand::RunSingleBacktest {
                config,
                symbols,
                trading_mode: app.strategy.trading_mode,
                initial_capital: app.strategy.initial_capital,
                position_size_pct: app.strategy.position_size_pct,
                start: app.sweep.config.start_date,
                end: app.sweep.config.end_date,
                cache_dir: app.cache_dir.clone(),
            });
            app.set_status("Running backtest...");
        }
        _ => {}
    }
}

fn adjust_strategy(app: &mut AppState, direction: i32) {
    let comp = app.strategy.active_component;
    let param = app.strategy.active_param;

    // Get the current variant's param ranges
    let variants = match comp {
        0 => &app.strategy.pool.signals,
        1 => &app.strategy.pool.position_managers,
        2 => &app.strategy.pool.execution_models,
        3 => &app.strategy.pool.filters,
        _ => return,
    };
    let idx = match comp {
        0 => &mut app.strategy.signal_idx,
        1 => &mut app.strategy.pm_idx,
        2 => &mut app.strategy.exec_idx,
        3 => &mut app.strategy.filter_idx,
        _ => return,
    };

    let current_variant = &variants[*idx];

    if current_variant.param_ranges.is_empty() || param >= current_variant.param_ranges.len() {
        // No params — cycle the component type
        let len = variants.len();
        if direction > 0 {
            *idx = (*idx + 1) % len;
        } else {
            *idx = (*idx + len - 1) % len;
        }
        app.strategy.reset_active_params();
        return;
    }

    // Adjust parameter value
    let range = &current_variant.param_ranges[param];
    let step = (range.max - range.min) / 20.0;
    let params = match comp {
        0 => &mut app.strategy.signal_params,
        1 => &mut app.strategy.pm_params,
        2 => &mut app.strategy.exec_params,
        3 => &mut app.strategy.filter_params,
        _ => return,
    };

    if param < params.len() {
        params[param] += step * direction as f64;
        params[param] = params[param].clamp(range.min, range.max);
    }
}

fn handle_sweep_key(app: &mut AppState, key: KeyEvent) {
    let setting_count = app.sweep.setting_count();

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if app.sweep.cursor + 1 < setting_count {
                app.sweep.cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.sweep.cursor = app.sweep.cursor.saturating_sub(1);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            adjust_sweep_setting(app, -1);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            adjust_sweep_setting(app, 1);
        }
        KeyCode::Enter => {
            if !app.sweep.yolo_running {
                if app.data.selected.is_empty() {
                    app.set_warning("Select tickers in Data panel first");
                    return;
                }
                let symbols: Vec<String> = app.data.selected.iter().cloned().collect();
                let mut config = app.sweep.config.clone();
                config.enforce_thread_constraints();
                let _ = app.worker_tx.send(crate::worker::WorkerCommand::StartYolo {
                    config,
                    symbols,
                    cache_dir: app.cache_dir.clone(),
                });
                app.sweep.yolo_running = true;
                app.set_status("YOLO mode started");
            }
        }
        KeyCode::Esc => {
            if app.sweep.yolo_running {
                app.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                app.set_warning("Stopping YOLO...");
            }
        }
        _ => {}
    }
}

fn adjust_sweep_setting(app: &mut AppState, direction: i32) {
    let c = &mut app.sweep.config;
    let d = direction as f64;
    match app.sweep.cursor {
        0 => c.jitter_pct = (c.jitter_pct + 0.05 * d).clamp(0.0, 1.0),
        1 => c.structural_explore = (c.structural_explore + 0.05 * d).clamp(0.0, 1.0),
        2 => {} // start_date — skip for now (needs date picker)
        3 => {} // end_date — skip for now
        4 => c.initial_capital = (c.initial_capital + 10_000.0 * d).max(1_000.0),
        5 => {} // fitness_metric — would cycle enum
        6 => {} // sweep_depth — would cycle enum
        7 => c.warmup_iterations = (c.warmup_iterations as i32 + direction * 10).max(0) as usize,
        8 => c.polars_thread_cap = (c.polars_thread_cap as i32 + direction).clamp(1, 16) as usize,
        9 => c.outer_thread_cap = (c.outer_thread_cap as i32 + direction).clamp(1, 16) as usize,
        10 => {
            let current = c.max_iterations.unwrap_or(0) as i32 + direction * 100;
            c.max_iterations = if current <= 0 { None } else { Some(current as usize) };
        }
        11 => c.master_seed = (c.master_seed as i64 + direction as i64).max(0) as u64,
        _ => {}
    }
    // Enforce thread constraints
    c.enforce_thread_constraints();
}

fn handle_results_key(app: &mut AppState, key: KeyEvent) {
    let entry_count = app.results.entries.len();

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if entry_count > 0 && app.results.cursor + 1 < entry_count {
                app.results.cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.results.cursor = app.results.cursor.saturating_sub(1);
        }
        KeyCode::Char('t') => {
            app.results.session_filter = match app.results.session_filter {
                SessionFilter::Session => SessionFilter::AllTime,
                SessionFilter::AllTime => SessionFilter::Session,
            };
        }
        KeyCode::Char('p') => {
            app.results.risk_profile = match app.results.risk_profile {
                RiskProfile::Balanced => RiskProfile::Conservative,
                RiskProfile::Conservative => RiskProfile::Aggressive,
                RiskProfile::Aggressive => RiskProfile::TrendOptions,
                RiskProfile::TrendOptions => RiskProfile::Balanced,
            };
        }
        KeyCode::Enter => {
            if !app.results.entries.is_empty() {
                // Open detail overlay and populate chart
                let idx = app.results.cursor;
                app.overlay = Overlay::Detail(idx);
            }
        }
        _ => {}
    }
}

fn handle_help_key(app: &mut AppState, key: KeyEvent) {
    if let KeyCode::Char('e') = key.code {
        app.overlay = Overlay::ErrorHistory;
        app.error_scroll = 0;
    }
}
