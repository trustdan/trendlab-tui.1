//! Application state — single-owner, main-thread only.
//!
//! All TUI state lives here. The worker thread communicates via channels.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use trendlab_core::components::sampler::{ComponentPool, ComponentVariant};
use trendlab_core::data::universe::Universe;
use trendlab_core::fingerprint::{ComponentConfig, StrategyConfig, TradingMode};
use trendlab_runner::{PerformanceMetrics, RiskProfile, YoloConfig, YoloProgress};

use crate::worker::{WorkerCommand, WorkerResponse};

/// Which panel is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    Data,
    Strategy,
    Sweep,
    Results,
    Chart,
    Help,
}

impl Panel {
    pub fn index(self) -> usize {
        match self {
            Panel::Data => 0,
            Panel::Strategy => 1,
            Panel::Sweep => 2,
            Panel::Results => 3,
            Panel::Chart => 4,
            Panel::Help => 5,
        }
    }

    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Panel::Data),
            1 => Some(Panel::Strategy),
            2 => Some(Panel::Sweep),
            3 => Some(Panel::Results),
            4 => Some(Panel::Chart),
            5 => Some(Panel::Help),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Panel::Data => "Data",
            Panel::Strategy => "Strategy",
            Panel::Sweep => "Sweep",
            Panel::Results => "Results",
            Panel::Chart => "Chart",
            Panel::Help => "Help",
        }
    }

    pub fn next(self) -> Panel {
        Panel::from_index((self.index() + 1) % 6).unwrap()
    }

    pub fn prev(self) -> Panel {
        Panel::from_index((self.index() + 5) % 6).unwrap()
    }
}

/// Status message severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Warning,
    Error,
}

/// An error record for the error history overlay.
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    pub timestamp: NaiveDateTime,
    pub category: ErrorCategory,
    pub message: String,
    pub context: String,
}

/// Error category for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Network,
    Data,
    Engine,
    NanMetrics,
    Other,
}

impl ErrorCategory {
    pub fn label(self) -> &'static str {
        match self {
            ErrorCategory::Network => "NET",
            ErrorCategory::Data => "DATA",
            ErrorCategory::Engine => "ENG",
            ErrorCategory::NanMetrics => "NaN",
            ErrorCategory::Other => "ERR",
        }
    }
}

/// Cursor position in the sector/ticker tree.
#[derive(Debug, Clone, Default)]
pub struct TreeCursor {
    /// Flat index into the visible tree rows.
    pub row: usize,
}

/// Data panel state.
#[derive(Debug)]
pub struct DataPanelState {
    pub universe: Universe,
    pub selected: HashSet<String>,
    pub expanded_sectors: HashSet<String>,
    pub cursor: TreeCursor,
    pub cache_status: HashMap<String, bool>,
    pub fetch_in_progress: bool,
    pub fetch_current_symbol: Option<String>,
    pub fetch_done: usize,
    pub fetch_total: usize,
}

impl DataPanelState {
    pub fn new(universe: Universe) -> Self {
        let expanded_sectors = universe.sector_names().into_iter().map(String::from).collect();
        Self {
            universe,
            selected: HashSet::new(),
            expanded_sectors,
            cursor: TreeCursor::default(),
            cache_status: HashMap::new(),
            fetch_in_progress: false,
            fetch_current_symbol: None,
            fetch_done: 0,
            fetch_total: 0,
        }
    }

    /// Count total visible rows (sectors + visible tickers).
    pub fn visible_row_count(&self) -> usize {
        let mut count = 0;
        for sector_name in self.universe.sector_names() {
            count += 1; // sector row
            if self.expanded_sectors.contains(sector_name) {
                if let Some(tickers) = self.universe.sector_tickers(sector_name) {
                    count += tickers.len();
                }
            }
        }
        count
    }

    /// Resolve the cursor row to either a sector name or a (sector, ticker) pair.
    pub fn cursor_item(&self) -> Option<TreeItem> {
        let mut row = 0;
        for sector_name in self.universe.sector_names() {
            if row == self.cursor.row {
                return Some(TreeItem::Sector(sector_name.to_string()));
            }
            row += 1;
            if self.expanded_sectors.contains(sector_name) {
                if let Some(tickers) = self.universe.sector_tickers(sector_name) {
                    for ticker in tickers {
                        if row == self.cursor.row {
                            return Some(TreeItem::Ticker(sector_name.to_string(), ticker.clone()));
                        }
                        row += 1;
                    }
                }
            }
        }
        None
    }
}

/// An item in the tree view.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TreeItem {
    Sector(String),
    Ticker(String, String), // (sector_name, ticker)
}

/// Strategy panel state — four-component composition builder.
pub struct StrategyPanelState {
    pub pool: ComponentPool,
    pub signal_idx: usize,
    pub pm_idx: usize,
    pub exec_idx: usize,
    pub filter_idx: usize,
    pub signal_params: Vec<f64>,
    pub pm_params: Vec<f64>,
    pub exec_params: Vec<f64>,
    pub filter_params: Vec<f64>,
    pub active_component: usize, // 0=signal, 1=pm, 2=exec, 3=filter
    pub active_param: usize,
    pub trading_mode: TradingMode,
    pub initial_capital: f64,
    pub position_size_pct: f64,
}

impl StrategyPanelState {
    pub fn new() -> Self {
        let pool = ComponentPool::default_pool();
        let signal_params = pool.signals[0]
            .param_ranges
            .iter()
            .map(|r| r.default)
            .collect();
        let pm_params = pool.position_managers[0]
            .param_ranges
            .iter()
            .map(|r| r.default)
            .collect();
        let exec_params = pool.execution_models[0]
            .param_ranges
            .iter()
            .map(|r| r.default)
            .collect();
        let filter_params = pool.filters[0]
            .param_ranges
            .iter()
            .map(|r| r.default)
            .collect();
        Self {
            pool,
            signal_idx: 0,
            pm_idx: 0,
            exec_idx: 0,
            filter_idx: 0,
            signal_params,
            pm_params,
            exec_params,
            filter_params,
            active_component: 0,
            active_param: 0,
            trading_mode: TradingMode::LongOnly,
            initial_capital: 100_000.0,
            position_size_pct: 100.0,
        }
    }

    /// Build a StrategyConfig from the current selections.
    pub fn to_strategy_config(&self) -> StrategyConfig {
        fn build_config(variant: &ComponentVariant, params: &[f64]) -> ComponentConfig {
            let mut map = std::collections::BTreeMap::new();
            for (range, val) in variant.param_ranges.iter().zip(params.iter()) {
                map.insert(range.name.clone(), *val);
            }
            ComponentConfig {
                component_type: variant.component_type.clone(),
                params: map,
            }
        }

        StrategyConfig {
            signal: build_config(&self.pool.signals[self.signal_idx], &self.signal_params),
            position_manager: build_config(
                &self.pool.position_managers[self.pm_idx],
                &self.pm_params,
            ),
            execution_model: build_config(
                &self.pool.execution_models[self.exec_idx],
                &self.exec_params,
            ),
            signal_filter: build_config(&self.pool.filters[self.filter_idx], &self.filter_params),
        }
    }

    /// Get the active component's variants.
    pub fn active_variants(&self) -> &[ComponentVariant] {
        match self.active_component {
            0 => &self.pool.signals,
            1 => &self.pool.position_managers,
            2 => &self.pool.execution_models,
            3 => &self.pool.filters,
            _ => &self.pool.signals,
        }
    }

    /// Get the active component's current index.
    pub fn active_idx(&self) -> usize {
        match self.active_component {
            0 => self.signal_idx,
            1 => self.pm_idx,
            2 => self.exec_idx,
            3 => self.filter_idx,
            _ => 0,
        }
    }

    /// Get the active component's param values.
    #[allow(dead_code)]
    pub fn active_params(&self) -> &[f64] {
        match self.active_component {
            0 => &self.signal_params,
            1 => &self.pm_params,
            2 => &self.exec_params,
            3 => &self.filter_params,
            _ => &self.signal_params,
        }
    }

    /// Reset params to defaults when component type changes.
    pub fn reset_active_params(&mut self) {
        let variant = match self.active_component {
            0 => &self.pool.signals[self.signal_idx],
            1 => &self.pool.position_managers[self.pm_idx],
            2 => &self.pool.execution_models[self.exec_idx],
            3 => &self.pool.filters[self.filter_idx],
            _ => return,
        };
        let defaults: Vec<f64> = variant.param_ranges.iter().map(|r| r.default).collect();
        match self.active_component {
            0 => self.signal_params = defaults,
            1 => self.pm_params = defaults,
            2 => self.exec_params = defaults,
            3 => self.filter_params = defaults,
            _ => {}
        }
        self.active_param = 0;
    }

    /// Total navigable rows: 4 component headers + their params.
    #[allow(dead_code)]
    pub fn total_rows(&self) -> usize {
        self.pool.signals[self.signal_idx].param_ranges.len()
            + self.pool.position_managers[self.pm_idx].param_ranges.len()
            + self.pool.execution_models[self.exec_idx].param_ranges.len()
            + self.pool.filters[self.filter_idx].param_ranges.len()
            + 4 // component headers
    }
}

/// Session vs all-time filter for results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionFilter {
    Session,
    AllTime,
}

/// A lightweight leaderboard display entry (no equity curve).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LeaderboardDisplayEntry {
    pub rank: usize,
    pub signal_type: String,
    pub pm_type: String,
    pub exec_type: String,
    pub filter_type: String,
    pub symbol: String,
    pub sharpe: f64,
    pub cagr: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub trade_count: usize,
    pub config: StrategyConfig,
    pub fitness_score: f64,
    pub session_id: String,
    /// Full metrics for drill-down.
    pub metrics: PerformanceMetrics,
    /// Stickiness metrics (if available).
    pub stickiness: Option<trendlab_core::engine::stickiness::StickinessMetrics>,
}

/// Results panel state.
pub struct ResultsPanelState {
    pub entries: Vec<LeaderboardDisplayEntry>,
    pub cursor: usize,
    pub session_filter: SessionFilter,
    pub risk_profile: RiskProfile,
    pub scroll_offset: usize,
    pub current_session_id: String,
}

impl ResultsPanelState {
    pub fn new(session_id: String) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            session_filter: SessionFilter::Session,
            risk_profile: RiskProfile::default(),
            scroll_offset: 0,
            current_session_id: session_id,
        }
    }
}

/// Sweep panel state — YOLO configuration.
pub struct SweepPanelState {
    pub config: YoloConfig,
    pub cursor: usize,
    pub yolo_running: bool,
    pub last_progress: Option<YoloProgress>,
}

impl SweepPanelState {
    pub fn new() -> Self {
        Self {
            config: YoloConfig::default(),
            cursor: 0,
            yolo_running: false,
            last_progress: None,
        }
    }

    /// Number of configurable settings.
    pub fn setting_count(&self) -> usize {
        12 // jitter, structural, start_date, end_date, initial_capital,
           // fitness_metric, sweep_depth, warmup_iters, polars_threads,
           // outer_threads, max_iterations, master_seed
    }
}

/// Chart panel state.
pub struct ChartPanelState {
    pub equity_curve: Option<Vec<f64>>,
    pub label: String,
}

impl ChartPanelState {
    pub fn new() -> Self {
        Self {
            equity_curve: None,
            label: String::new(),
        }
    }
}

/// Which overlay (if any) is shown on top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    None,
    Welcome,
    Detail(usize),     // index into results entries
    ErrorHistory,
    Search,
}

/// Top-level application state.
pub struct AppState {
    // Navigation
    pub active_panel: Panel,
    pub running: bool,

    // Panel states
    pub data: DataPanelState,
    pub strategy: StrategyPanelState,
    pub sweep: SweepPanelState,
    pub results: ResultsPanelState,
    pub chart: ChartPanelState,

    // Worker communication
    pub worker_tx: Sender<WorkerCommand>,
    pub worker_rx: Receiver<WorkerResponse>,
    pub cancel: Arc<AtomicBool>,

    // Cross-cutting
    pub status_message: Option<(String, StatusLevel)>,
    pub error_history: VecDeque<ErrorRecord>,
    pub error_scroll: usize,
    pub overlay: Overlay,
    pub search_input: String,

    // Paths
    pub cache_dir: PathBuf,
    #[allow(dead_code)]
    pub state_path: PathBuf,
}

impl AppState {
    pub fn new(
        worker_tx: Sender<WorkerCommand>,
        worker_rx: Receiver<WorkerResponse>,
        cancel: Arc<AtomicBool>,
        cache_dir: PathBuf,
        state_path: PathBuf,
    ) -> Self {
        let universe = Universe::default_us();
        let session_id = format!("s_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"));
        Self {
            active_panel: Panel::Data,
            running: true,
            data: DataPanelState::new(universe),
            strategy: StrategyPanelState::new(),
            sweep: SweepPanelState::new(),
            results: ResultsPanelState::new(session_id),
            chart: ChartPanelState::new(),
            worker_tx,
            worker_rx,
            cancel,
            status_message: None,
            error_history: VecDeque::with_capacity(50),
            error_scroll: 0,
            overlay: Overlay::None,
            search_input: String::new(),
            cache_dir,
            state_path,
        }
    }

    /// Push an error to the history, capping at 50.
    pub fn push_error(&mut self, category: ErrorCategory, message: String, context: String) {
        let record = ErrorRecord {
            timestamp: chrono::Local::now().naive_local(),
            category,
            message: message.clone(),
            context,
        };
        self.error_history.push_front(record);
        if self.error_history.len() > 50 {
            self.error_history.pop_back();
        }
        self.status_message = Some((message, StatusLevel::Error));
    }

    /// Set an info status message.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), StatusLevel::Info));
    }

    /// Set a warning status message.
    pub fn set_warning(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), StatusLevel::Warning));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_cycle() {
        assert_eq!(Panel::Data.next(), Panel::Strategy);
        assert_eq!(Panel::Help.next(), Panel::Data);
        assert_eq!(Panel::Data.prev(), Panel::Help);
        assert_eq!(Panel::Strategy.prev(), Panel::Data);
    }

    #[test]
    fn panel_from_index() {
        for i in 0..6 {
            let p = Panel::from_index(i).unwrap();
            assert_eq!(p.index(), i);
        }
        assert!(Panel::from_index(6).is_none());
    }

    #[test]
    fn error_history_caps_at_50() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let (_tx2, rx2) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut app = AppState::new(
            tx,
            rx2,
            cancel,
            PathBuf::from("."),
            PathBuf::from("."),
        );
        for i in 0..60 {
            app.push_error(ErrorCategory::Other, format!("error {i}"), String::new());
        }
        assert_eq!(app.error_history.len(), 50);
        assert!(app.error_history[0].message.contains("59"));
    }

    #[test]
    fn strategy_builds_valid_config() {
        let state = StrategyPanelState::new();
        let config = state.to_strategy_config();
        assert!(!config.signal.component_type.is_empty());
        assert!(!config.position_manager.component_type.is_empty());
        assert!(!config.execution_model.component_type.is_empty());
        assert!(!config.signal_filter.component_type.is_empty());
    }

    #[test]
    fn data_panel_visible_rows() {
        let universe = Universe::default_us();
        let state = DataPanelState::new(universe);
        // All sectors expanded: 6 sectors + all their tickers
        let total = state.visible_row_count();
        assert!(total > 30); // 6 sectors + ~54 tickers
    }

    #[test]
    fn data_panel_cursor_item() {
        let universe = Universe::default_us();
        let mut state = DataPanelState::new(universe);
        // First row should be a sector
        state.cursor.row = 0;
        match state.cursor_item() {
            Some(TreeItem::Sector(_)) => {}
            other => panic!("Expected Sector, got {:?}", other),
        }
        // Second row (sector expanded) should be a ticker
        state.cursor.row = 1;
        match state.cursor_item() {
            Some(TreeItem::Ticker(_, _)) => {}
            other => panic!("Expected Ticker, got {:?}", other),
        }
    }
}
