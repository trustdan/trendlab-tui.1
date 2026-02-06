//! TUI application state and lifecycle management
//!
//! Manages:
//! - Global app state (current panel, drill-down state)
//! - Backtest results (strategy results from runner)
//! - Navigation state machine
//! - Event handling delegation

use crate::backtest_service::BacktestService;
use crate::drill_down::{DrillDownState, SummaryCardData, DiagnosticData};
use crate::ghost_curve::{GhostCurve, IdealEquity, RealEquity};
use crate::panels::{TradeRecord, TradeMarker, RejectedIntentRecord, RejectionStats, ExecutionPreset, PresetState};
use crate::theme::Theme;
use trendlab_runner::config::{
    ExecutionConfig, SlippageConfig, CommissionConfig, IntrabarPolicy,
};
use trendlab_runner::result::{BacktestResult, TradeDirection, TradeRecord as RunnerTradeRecord, EquityPoint};
use trendlab_runner::FitnessMetric;
use trendlab_runner::LevelResult;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use serde_json::Value;

/// Message from a background rerun thread
struct RerunMessage {
    run_id: String,
    preset_name: String,
    result: Result<BacktestResult, String>,
}

/// Stored state for a rerun attempt
#[derive(Debug, Clone)]
pub enum RerunState {
    Running,
    Complete(Box<BacktestResult>),
    Failed(String),
}

/// Chart display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartMode {
    EquityCurve,
    CandleChart,
}

/// Main TUI application state
pub struct App {
    /// Current theme
    pub theme: Theme,
    /// Current drill-down state (which panel/view is active)
    pub drill_down: DrillDownState,
    /// Sorted backtest results (top strategies)
    pub results: Vec<BacktestResult>,
    /// Results indexed by run_id for quick lookup
    pub results_by_id: HashMap<String, BacktestResult>,
    /// Currently selected row in leaderboard (0-indexed)
    pub selected_index: usize,
    /// Whether the app should exit
    pub should_quit: bool,
    /// Error message to display (if any)
    pub error_message: Option<String>,
    /// Current fitness metric for leaderboard sorting
    pub fitness_metric: FitnessMetric,
    /// When this session started (for session/all-time filtering)
    pub session_start: DateTime<Utc>,
    /// Whether to show only session results
    pub show_session_only: bool,
    /// Backtest service for triggering reruns
    backtest_service: Option<Arc<dyn BacktestService>>,
    /// Rerun results keyed by "run_id:preset_name"
    rerun_states: HashMap<String, RerunState>,
    /// Channel receiver for completed reruns
    rerun_receiver: Option<mpsc::Receiver<RerunMessage>>,
    /// Channel sender for spawning reruns (cloned into threads)
    rerun_sender: Option<mpsc::Sender<RerunMessage>>,
    /// Current chart display mode
    pub chart_mode: ChartMode,
}

impl App {
    /// Create a new app with empty state
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            theme: Theme::default(),
            drill_down: DrillDownState::Leaderboard,
            results: Vec::new(),
            results_by_id: HashMap::new(),
            selected_index: 0,
            should_quit: false,
            error_message: None,
            fitness_metric: FitnessMetric::Sharpe,
            session_start: Utc::now(),
            show_session_only: false,
            backtest_service: None,
            rerun_states: HashMap::new(),
            rerun_receiver: Some(rx),
            rerun_sender: Some(tx),
            chart_mode: ChartMode::EquityCurve,
        }
    }

    /// Set the backtest service for triggering reruns
    pub fn set_backtest_service(&mut self, service: Arc<dyn BacktestService>) {
        self.backtest_service = Some(service);
    }

    /// Load backtest results
    pub fn load_results(&mut self, results: Vec<BacktestResult>) {
        // Build index by run_id
        self.results_by_id = results
            .iter()
            .map(|r| (r.run_id.clone(), r.clone()))
            .collect();

        self.results = results;
        self.selected_index = 0;
    }

    /// Get the currently selected run ID (from leaderboard)
    pub fn selected_run_id(&self) -> Option<&str> {
        self.results
            .get(self.selected_index)
            .map(|result| result.run_id.as_str())
    }

    /// Get the currently selected result
    pub fn selected_result(&self) -> Option<&BacktestResult> {
        self.results.get(self.selected_index)
    }

    /// Build summary card data for a run
    pub fn summary_card_data(&self, run_id: &str) -> Option<SummaryCardData> {
        let result = self.results_by_id.get(run_id)?;
        Some(SummaryCardData {
            run_id: result.run_id.clone(),
            strategy_name: self
                .metadata_string(result, "strategy_name")
                .unwrap_or_else(|| result.run_id.chars().take(12).collect()),
            sharpe: result.stats.sharpe,
            total_return: result.stats.total_return,
            max_drawdown: result.stats.max_drawdown,
            win_rate: result.stats.win_rate,
            trade_count: result.stats.num_trades,
            avg_trade_duration_days: result.stats.avg_duration_days,
            profit_factor: result.stats.profit_factor,
        })
    }

    /// Build trade tape records for a run
    pub fn trade_records(&self, run_id: &str) -> Vec<TradeRecord> {
        let result = match self.results_by_id.get(run_id) {
            Some(result) => result,
            None => return Vec::new(),
        };

        result
            .trades
            .iter()
            .enumerate()
            .map(|(idx, trade)| TradeRecord {
                trade_id: format!("trade_{}", idx),
                symbol: trade.symbol.clone(),
                direction: match trade.direction {
                    TradeDirection::Long => "Long".to_string(),
                    TradeDirection::Short => "Short".to_string(),
                },
                entry_date: trade.entry_date.to_string(),
                exit_date: trade.exit_date.to_string(),
                pnl: trade.pnl,
                duration_days: self.trade_duration_days(trade),
                signal_intent: trade.signal_intent.clone(),
                order_type: trade.order_type.clone(),
                fill_context: trade.fill_context.clone(),
            })
            .collect()
    }

    /// Build trade markers for chart rendering
    pub fn trade_markers(&self, run_id: &str) -> Vec<TradeMarker> {
        let result = match self.results_by_id.get(run_id) {
            Some(result) => result,
            None => return Vec::new(),
        };

        let mut date_index = HashMap::new();
        for (idx, point) in result.equity_curve.iter().enumerate() {
            date_index.insert(point.date, idx);
        }

        let mut markers = Vec::new();
        for (idx, trade) in result.trades.iter().enumerate() {
            if let Some(entry_idx) = date_index.get(&trade.entry_date) {
                markers.push(TradeMarker {
                    bar_index: *entry_idx,
                    price: trade.entry_price,
                    label: format!("E{}", idx + 1),
                });
            }
            if let Some(exit_idx) = date_index.get(&trade.exit_date) {
                markers.push(TradeMarker {
                    bar_index: *exit_idx,
                    price: trade.exit_price,
                    label: format!("X{}", idx + 1),
                });
            }
        }

        markers
    }

    /// Build ghost curve from the run's equity curve and optional ideal curve metadata
    pub fn ghost_curve(&self, run_id: &str) -> Option<GhostCurve> {
        let result = self.results_by_id.get(run_id)?;
        let real = self.real_equity_from_points(&result.equity_curve);
        let ideal = self
            .ideal_equity_from_metadata(result)
            .unwrap_or_else(|| self.ideal_equity_from_real(&real));

        Some(GhostCurve::new(ideal, real))
    }

    /// Build diagnostics data for a specific trade
    pub fn diagnostics_for_trade(&self, run_id: &str, trade_id: &str) -> Option<DiagnosticData> {
        let result = self.results_by_id.get(run_id)?;
        let trade_index = trade_id
            .strip_prefix("trade_")
            .and_then(|val| val.parse::<usize>().ok())
            .or(Some(self.selected_index))?;
        let trade = result.trades.get(trade_index)?;
        let entry_bar = result
            .equity_curve
            .iter()
            .position(|point| point.date == trade.entry_date)
            .unwrap_or(0);
        let exit_bar = result
            .equity_curve
            .iter()
            .position(|point| point.date == trade.exit_date)
            .unwrap_or(entry_bar);

        let entry_slippage = trade.entry_slippage.unwrap_or(0.0);
        let exit_slippage = trade.exit_slippage.unwrap_or(0.0);
        let entry_gap_fill = trade.entry_was_gapped.unwrap_or(false);
        let exit_gap_fill = trade.exit_was_gapped.unwrap_or(false);

        // Fill price = ideal price + slippage (for longs, slippage makes entry worse)
        let entry_fill_price = trade.entry_price + entry_slippage;
        let exit_fill_price = trade.exit_price - exit_slippage;

        Some(DiagnosticData {
            trade_id: trade_id.to_string(),
            symbol: trade.symbol.clone(),
            entry_bar,
            entry_ideal_price: trade.entry_price,
            entry_fill_price,
            entry_slippage,
            entry_gap_fill,
            exit_bar,
            exit_ideal_price: trade.exit_price,
            exit_fill_price,
            exit_slippage,
            exit_gap_fill,
            ambiguity_note: None,
            signal_intent: trade.signal_intent.clone(),
            order_type: trade.order_type.clone(),
            fill_context: trade.fill_context.clone(),
        })
    }

    /// Build rejected intent records and stats for a run
    pub fn rejected_intents(&self, run_id: &str) -> (Vec<RejectedIntentRecord>, RejectionStats) {
        let result = match self.results_by_id.get(run_id) {
            Some(result) => result,
            None => {
                return (
                    Vec::new(),
                    RejectionStats {
                        total_signals: 0,
                        total_rejected: 0,
                        by_reason: Self::default_rejection_reasons(),
                    },
                );
            }
        };

        let records = self.rejected_intents_from_metadata(result);
        let mut counts = HashMap::new();
        for reason in ["VolatilityGuard", "LiquidityGuard", "MarginGuard", "RiskGuard"] {
            counts.insert(reason.to_string(), 0usize);
        }

        for record in &records {
            *counts.entry(record.rejection_reason.clone()).or_insert(0) += 1;
        }

        let total_rejected = records.len();
        let total_signals = self
            .metadata_usize(result, "total_signals")
            .unwrap_or(total_rejected);

        let by_reason = ["VolatilityGuard", "LiquidityGuard", "MarginGuard", "RiskGuard"]
            .iter()
            .map(|reason| {
                (
                    reason.to_string(),
                    *counts.get(*reason).unwrap_or(&0),
                )
            })
            .collect();

        (
            records,
            RejectionStats {
                total_signals,
                total_rejected,
                by_reason,
            },
        )
    }

    /// Build execution lab presets for a run, reflecting current rerun state
    pub fn execution_presets(&self, run_id: &str) -> Vec<ExecutionPreset> {
        let result = match self.results_by_id.get(run_id) {
            Some(result) => result,
            None => return Vec::new(),
        };

        let preset_defs = [
            ("Deterministic", "Baseline execution (5bps slippage, WorstCase)"),
            ("WorstCase", "Conservative: 10bps slippage, adverse ordering"),
            ("BestCase", "Optimistic: 2bps slippage, favorable ordering"),
            ("PathMC", "Monte Carlo intrabar sampling (OhlcOrder)"),
        ];

        preset_defs
            .iter()
            .map(|(name, desc)| {
                let key = format!("{}:{}", run_id, name);
                match self.rerun_states.get(&key) {
                    Some(RerunState::Running) => ExecutionPreset {
                        name: name.to_string(),
                        description: desc.to_string(),
                        sharpe: None,
                        total_return: None,
                        state: PresetState::Running,
                    },
                    Some(RerunState::Complete(res)) => ExecutionPreset {
                        name: name.to_string(),
                        description: desc.to_string(),
                        sharpe: Some(res.stats.sharpe),
                        total_return: Some(res.stats.total_return),
                        state: PresetState::Complete,
                    },
                    Some(RerunState::Failed(err)) => ExecutionPreset {
                        name: name.to_string(),
                        description: desc.to_string(),
                        sharpe: None,
                        total_return: None,
                        state: PresetState::Failed(err.clone()),
                    },
                    None => {
                        // Deterministic preset shows the base result
                        if *name == "Deterministic" {
                            ExecutionPreset {
                                name: name.to_string(),
                                description: desc.to_string(),
                                sharpe: Some(result.stats.sharpe),
                                total_return: Some(result.stats.total_return),
                                state: PresetState::Complete,
                            }
                        } else {
                            ExecutionPreset {
                                name: name.to_string(),
                                description: desc.to_string(),
                                sharpe: None,
                                total_return: None,
                                state: PresetState::NotRun,
                            }
                        }
                    }
                }
            })
            .collect()
    }

    /// Returns results sorted by the current fitness metric, filtered by session if active.
    pub fn sorted_results(&self) -> Vec<&BacktestResult> {
        let mut filtered: Vec<&BacktestResult> = if self.show_session_only {
            self.results
                .iter()
                .filter(|r| r.metadata.timestamp >= self.session_start)
                .collect()
        } else {
            self.results.iter().collect()
        };

        filtered.sort_by(|a, b| {
            let score_a = self.fitness_metric.extract(a);
            let score_b = self.fitness_metric.extract(b);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        filtered
    }

    /// Cycle to the next fitness metric
    pub fn cycle_fitness_metric(&mut self) {
        self.fitness_metric = match self.fitness_metric {
            FitnessMetric::Sharpe => FitnessMetric::Sortino,
            FitnessMetric::Sortino => FitnessMetric::Calmar,
            FitnessMetric::Calmar => FitnessMetric::TotalReturn,
            FitnessMetric::TotalReturn => FitnessMetric::AnnualReturn,
            FitnessMetric::AnnualReturn => FitnessMetric::WinRate,
            FitnessMetric::WinRate => FitnessMetric::ProfitFactor,
            FitnessMetric::ProfitFactor => FitnessMetric::Composite,
            FitnessMetric::Composite => FitnessMetric::Sharpe,
        };
    }

    /// Toggle session-only vs all-time leaderboard
    pub fn toggle_session_filter(&mut self) {
        self.show_session_only = !self.show_session_only;
    }

    /// Trigger a rerun for the given run_id with the named preset.
    /// Spawns a background thread; result arrives via poll_reruns().
    pub fn trigger_rerun(&mut self, run_id: &str, preset_name: &str) {
        let service = match &self.backtest_service {
            Some(s) => Arc::clone(s),
            None => {
                self.set_error("No backtest service configured".to_string());
                return;
            }
        };

        let base_config = match self
            .results_by_id
            .get(run_id)
            .and_then(|r| r.metadata.config.clone())
        {
            Some(cfg) => cfg,
            None => {
                self.set_error("No config stored in result metadata".to_string());
                return;
            }
        };

        let execution = Self::execution_config_for_preset(preset_name);
        let key = format!("{}:{}", run_id, preset_name);
        self.rerun_states.insert(key, RerunState::Running);

        let sender = match &self.rerun_sender {
            Some(s) => s.clone(),
            None => return,
        };
        let run_id_owned = run_id.to_string();
        let preset_owned = preset_name.to_string();

        std::thread::spawn(move || {
            let result = service
                .rerun_with_execution(&base_config, &execution)
                .map_err(|e| e.to_string());
            let _ = sender.send(RerunMessage {
                run_id: run_id_owned,
                preset_name: preset_owned,
                result,
            });
        });
    }

    /// Poll for completed reruns. Call this each tick in the event loop.
    pub fn poll_reruns(&mut self) {
        let receiver = match &self.rerun_receiver {
            Some(r) => r,
            None => return,
        };

        // Drain all available messages (non-blocking)
        while let Ok(msg) = receiver.try_recv() {
            let key = format!("{}:{}", msg.run_id, msg.preset_name);
            match msg.result {
                Ok(result) => {
                    self.rerun_states
                        .insert(key, RerunState::Complete(Box::new(result)));
                }
                Err(err) => {
                    self.rerun_states.insert(key, RerunState::Failed(err));
                }
            }
        }
    }

    /// Returns the ExecutionConfig for a named preset.
    pub fn execution_config_for_preset(preset_name: &str) -> ExecutionConfig {
        match preset_name {
            "Deterministic" => ExecutionConfig {
                slippage: SlippageConfig::FixedBps { bps: 5.0 },
                commission: CommissionConfig::PerShare { amount: 0.005 },
                intrabar_policy: IntrabarPolicy::WorstCase,
            },
            "WorstCase" => ExecutionConfig {
                slippage: SlippageConfig::FixedBps { bps: 10.0 },
                commission: CommissionConfig::PerShare { amount: 0.005 },
                intrabar_policy: IntrabarPolicy::WorstCase,
            },
            "BestCase" => ExecutionConfig {
                slippage: SlippageConfig::FixedBps { bps: 2.0 },
                commission: CommissionConfig::PerShare { amount: 0.005 },
                intrabar_policy: IntrabarPolicy::BestCase,
            },
            "PathMC" => ExecutionConfig {
                slippage: SlippageConfig::FixedBps { bps: 5.0 },
                commission: CommissionConfig::PerShare { amount: 0.005 },
                intrabar_policy: IntrabarPolicy::OhlcOrder,
            },
            _ => ExecutionConfig::default(),
        }
    }

    /// Trigger rerun for the currently selected preset in ExecutionLab.
    /// Called when Enter is pressed in ExecutionLab state.
    pub fn trigger_selected_rerun(&mut self) {
        if let DrillDownState::ExecutionLab(run_id) = &self.drill_down {
            let run_id = run_id.clone();
            let presets = self.execution_presets(&run_id);
            if let Some(preset) = presets.get(self.selected_index) {
                let name = preset.name.clone();
                self.trigger_rerun(&run_id, &name);
            }
        }
    }

    /// Navigate to next item in current view
    pub fn select_next(&mut self) {
        match &self.drill_down {
            DrillDownState::Leaderboard => {
                if !self.results.is_empty() {
                    self.selected_index = (self.selected_index + 1).min(self.results.len() - 1);
                }
            }
            DrillDownState::TradeTape(run_id) => {
                if let Some(result) = self.results_by_id.get(run_id) {
                    let trade_count = result.trades.len();
                    if trade_count > 0 {
                        self.selected_index = (self.selected_index + 1).min(trade_count - 1);
                    }
                }
            }
            DrillDownState::ExecutionLab(_) => {
                // 4 presets
                self.selected_index = (self.selected_index + 1).min(3);
            }
            _ => {}
        }
    }

    /// Navigate to previous item in current view
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    /// Drill down into selected item (Enter key)
    pub fn drill_down(&mut self) {
        match &self.drill_down {
            DrillDownState::Leaderboard => {
                if let Some(run_id) = self.selected_run_id() {
                    self.drill_down = DrillDownState::SummaryCard(run_id.to_string());
                }
            }
            DrillDownState::SummaryCard(run_id) => {
                self.selected_index = 0; // Reset selection for trade tape
                self.drill_down = DrillDownState::TradeTape(run_id.clone());
            }
            DrillDownState::TradeTape(run_id) => {
                if let Some(result) = self.results_by_id.get(run_id) {
                    if result.trades.get(self.selected_index).is_some() {
                        let trade_id = format!("trade_{}", self.selected_index);
                        self.drill_down = DrillDownState::ChartWithTrade(
                            run_id.clone(),
                            trade_id,
                        );
                    }
                }
            }
            DrillDownState::ExecutionLab(_) => {
                self.trigger_selected_rerun();
            }
            _ => {}
        }
    }

    /// Navigate back to previous view (Esc/Backspace)
    pub fn go_back(&mut self) {
        match &self.drill_down {
            DrillDownState::SummaryCard(_) => {
                self.drill_down = DrillDownState::Leaderboard;
            }
            DrillDownState::TradeTape(run_id) => {
                self.drill_down = DrillDownState::SummaryCard(run_id.clone());
            }
            DrillDownState::ChartWithTrade(run_id, _) => {
                self.drill_down = DrillDownState::TradeTape(run_id.clone());
            }
            DrillDownState::Diagnostics(run_id, trade_id) => {
                self.drill_down = DrillDownState::ChartWithTrade(run_id.clone(), trade_id.clone());
            }
            DrillDownState::RejectedIntents(run_id) => {
                self.drill_down = DrillDownState::SummaryCard(run_id.clone());
            }
            DrillDownState::ExecutionLab(run_id) => {
                self.drill_down = DrillDownState::SummaryCard(run_id.clone());
            }
            DrillDownState::Sensitivity(run_id) => {
                self.drill_down = DrillDownState::SummaryCard(run_id.clone());
            }
            DrillDownState::RunManifest(run_id) => {
                self.drill_down = DrillDownState::SummaryCard(run_id.clone());
            }
            DrillDownState::Robustness(run_id) => {
                self.drill_down = DrillDownState::SummaryCard(run_id.clone());
            }
            DrillDownState::Leaderboard => {
                // Already at top level, do nothing
            }
        }
    }

    /// Show diagnostics for current trade (d key)
    pub fn show_diagnostics(&mut self) {
        if let DrillDownState::ChartWithTrade(run_id, trade_id) = &self.drill_down {
            self.drill_down = DrillDownState::Diagnostics(run_id.clone(), trade_id.clone());
        }
    }

    /// Show rejected intents (i key)
    pub fn show_rejected_intents(&mut self) {
        if let DrillDownState::SummaryCard(run_id) = &self.drill_down {
            self.drill_down = DrillDownState::RejectedIntents(run_id.clone());
        }
    }

    /// Show execution lab (r key)
    pub fn show_execution_lab(&mut self) {
        if let DrillDownState::SummaryCard(run_id) = &self.drill_down {
            self.drill_down = DrillDownState::ExecutionLab(run_id.clone());
        }
    }

    /// Show sensitivity panel (x key) — accessible from SummaryCard or ExecutionLab
    pub fn show_sensitivity(&mut self) {
        let run_id = match &self.drill_down {
            DrillDownState::SummaryCard(run_id) | DrillDownState::ExecutionLab(run_id) => {
                run_id.clone()
            }
            _ => return,
        };
        self.drill_down = DrillDownState::Sensitivity(run_id);
    }

    /// Show run manifest (m key) — accessible from SummaryCard
    pub fn show_manifest(&mut self) {
        if let DrillDownState::SummaryCard(run_id) = &self.drill_down {
            self.drill_down = DrillDownState::RunManifest(run_id.clone());
        }
    }

    /// Show robustness ladder (b key) — accessible from SummaryCard
    pub fn show_robustness(&mut self) {
        if let DrillDownState::SummaryCard(run_id) = &self.drill_down {
            self.drill_down = DrillDownState::Robustness(run_id.clone());
        }
    }

    /// Toggle chart mode between equity curve and candle chart (c key)
    pub fn toggle_chart_mode(&mut self) {
        self.chart_mode = match self.chart_mode {
            ChartMode::EquityCurve => ChartMode::CandleChart,
            ChartMode::CandleChart => ChartMode::EquityCurve,
        };
    }

    /// Get completed rerun results for a run_id (for sensitivity panel).
    /// Returns a map of "run_id:preset_name" -> BacktestResult for Complete states.
    pub fn completed_reruns(&self, run_id: &str) -> HashMap<String, Box<BacktestResult>> {
        self.rerun_states
            .iter()
            .filter(|(key, _)| key.starts_with(&format!("{}:", run_id)))
            .filter_map(|(key, state)| match state {
                RerunState::Complete(result) => Some((key.clone(), result.clone())),
                _ => None,
            })
            .collect()
    }

    /// Get robustness level results for a run.
    /// Returns stored robustness data from metadata, or empty if none available.
    pub fn robustness_levels(&self, run_id: &str) -> Vec<LevelResult> {
        let result = match self.results_by_id.get(run_id) {
            Some(r) => r,
            None => return Vec::new(),
        };

        // Try to deserialize from metadata.custom["robustness_levels"]
        if let Some(value) = result.metadata.custom.get("robustness_levels") {
            if let Ok(levels) = serde_json::from_value::<Vec<LevelResult>>(value.clone()) {
                return levels;
            }
        }

        Vec::new()
    }

    /// Request app exit
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Set error message
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
    }

    /// Clear error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    fn trade_duration_days(&self, trade: &RunnerTradeRecord) -> u32 {
        let days = (trade.exit_date - trade.entry_date).num_days();
        if days < 0 {
            0
        } else {
            days as u32
        }
    }

    fn ideal_equity_from_metadata(&self, result: &BacktestResult) -> Option<IdealEquity> {
        let custom = &result.metadata.custom;
        let values = custom.get("ideal_equity_curve")?;
        let Value::Array(points) = values else { return None; };

        let mut ideal = IdealEquity::new();
        for point in points {
            if let Value::Object(map) = point {
                let date = map.get("date").and_then(|val| val.as_str())?;
                let equity = map.get("equity").and_then(|val| val.as_f64())?;
                let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
                let ts = Utc.from_utc_datetime(&date.and_hms_opt(16, 0, 0)?);
                ideal.push(ts, equity);
            }
        }

        if ideal.is_empty() {
            None
        } else {
            Some(ideal)
        }
    }

    fn ideal_equity_from_real(&self, real: &RealEquity) -> IdealEquity {
        let mut ideal = IdealEquity::new();
        for (idx, value) in real.values.iter().enumerate() {
            if let Some(ts) = real.timestamps.get(idx) {
                ideal.push(*ts, *value);
            }
        }
        ideal
    }

    fn real_equity_from_points(&self, points: &[EquityPoint]) -> RealEquity {
        let mut real = RealEquity::new();
        for point in points {
            if let Some(ts) = point
                .date
                .and_hms_opt(16, 0, 0)
                .map(|dt| Utc.from_utc_datetime(&dt))
            {
                real.push(ts, point.equity);
            }
        }
        real
    }

    fn rejected_intents_from_metadata(&self, result: &BacktestResult) -> Vec<RejectedIntentRecord> {
        let mut records = Vec::new();
        let custom = &result.metadata.custom;
        let Some(Value::Array(entries)) = custom.get("rejected_intents") else {
            return records;
        };

        for entry in entries {
            if let Value::Object(map) = entry {
                let bar_index = map.get("bar_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let date = map.get("date").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let signal = map.get("signal").and_then(|v| v.as_str()).unwrap_or("Long").to_string();
                let rejection_reason = map
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("VolatilityGuard")
                    .to_string();
                let context = map.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();

                records.push(RejectedIntentRecord {
                    bar_index,
                    date,
                    signal,
                    rejection_reason,
                    context,
                });
            }
        }

        records
    }

    fn metadata_string(&self, result: &BacktestResult, key: &str) -> Option<String> {
        result
            .metadata
            .custom
            .get(key)
            .and_then(|val| val.as_str())
            .map(|val| val.to_string())
    }

    fn metadata_usize(&self, result: &BacktestResult, key: &str) -> Option<usize> {
        result
            .metadata
            .custom
            .get(key)
            .and_then(|val| val.as_u64())
            .map(|val| val as usize)
    }

    fn default_rejection_reasons() -> Vec<(String, usize)> {
        vec![
            ("VolatilityGuard".to_string(), 0),
            ("LiquidityGuard".to_string(), 0),
            ("MarginGuard".to_string(), 0),
            ("RiskGuard".to_string(), 0),
        ]
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_result;

    fn create_test_app() -> App {
        let mut app = App::new();
        let results = vec![create_test_result("run_1", 2.5)];
        app.load_results(results);
        app
    }

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert_eq!(app.selected_index, 0);
        assert!(!app.should_quit);
        assert_eq!(app.error_message, None);
    }

    #[test]
    fn test_select_next_in_leaderboard() {
        let mut app = create_test_app();
        assert_eq!(app.selected_index, 0);

        // Only one result, so should stay at 0
        app.select_next();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_select_previous() {
        let mut app = create_test_app();
        app.selected_index = 0;

        app.select_previous();
        assert_eq!(app.selected_index, 0); // Should not go below 0
    }

    #[test]
    fn test_drill_down_from_leaderboard() {
        let mut app = create_test_app();
        assert!(matches!(app.drill_down, DrillDownState::Leaderboard));

        app.drill_down();
        assert!(matches!(app.drill_down, DrillDownState::SummaryCard(_)));
    }

    #[test]
    fn test_go_back_from_summary_card() {
        let mut app = create_test_app();
        app.drill_down = DrillDownState::SummaryCard("run_1".to_string());

        app.go_back();
        assert!(matches!(app.drill_down, DrillDownState::Leaderboard));
    }

    #[test]
    fn test_show_diagnostics_from_chart() {
        let mut app = create_test_app();
        app.drill_down = DrillDownState::ChartWithTrade("run_1".to_string(), "trade_1".to_string());

        app.show_diagnostics();
        assert!(matches!(app.drill_down, DrillDownState::Diagnostics(_, _)));
    }

    #[test]
    fn test_show_rejected_intents_from_summary() {
        let mut app = create_test_app();
        app.drill_down = DrillDownState::SummaryCard("run_1".to_string());

        app.show_rejected_intents();
        assert!(matches!(app.drill_down, DrillDownState::RejectedIntents(_)));
    }

    #[test]
    fn test_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);

        app.quit();
        assert!(app.should_quit);
    }

    #[test]
    fn test_error_handling() {
        let mut app = App::new();
        assert_eq!(app.error_message, None);

        app.set_error("Test error".to_string());
        assert_eq!(app.error_message, Some("Test error".to_string()));

        app.clear_error();
        assert_eq!(app.error_message, None);
    }

    #[test]
    fn test_cycle_fitness_metric() {
        let mut app = App::new();
        assert_eq!(app.fitness_metric, FitnessMetric::Sharpe);

        app.cycle_fitness_metric();
        assert_eq!(app.fitness_metric, FitnessMetric::Sortino);

        app.cycle_fitness_metric();
        assert_eq!(app.fitness_metric, FitnessMetric::Calmar);

        // Cycle all the way around
        for _ in 0..6 {
            app.cycle_fitness_metric();
        }
        assert_eq!(app.fitness_metric, FitnessMetric::Sharpe);
    }

    #[test]
    fn test_toggle_session_filter() {
        let mut app = App::new();
        assert!(!app.show_session_only);

        app.toggle_session_filter();
        assert!(app.show_session_only);

        app.toggle_session_filter();
        assert!(!app.show_session_only);
    }

    #[test]
    fn test_sorted_results_by_metric() {
        let mut app = App::new();
        let mut r1 = create_test_result("run_1", 2.5);
        r1.stats.win_rate = 0.40;
        let mut r2 = create_test_result("run_2", 1.0);
        r2.stats.win_rate = 0.80;
        app.load_results(vec![r1, r2]);

        // By Sharpe: run_1 first
        let sorted = app.sorted_results();
        assert_eq!(sorted[0].run_id, "run_1");

        // By WinRate: run_2 first
        app.fitness_metric = FitnessMetric::WinRate;
        let sorted = app.sorted_results();
        assert_eq!(sorted[0].run_id, "run_2");
    }
}
