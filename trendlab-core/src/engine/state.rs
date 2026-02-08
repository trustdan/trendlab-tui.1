//! Engine configuration, mutable state, and run result types.

use crate::components::signal::{SignalEvaluation, SignalEvent};
use crate::domain::ids::IdGen;
use crate::domain::{Fill, Instrument, OrderId, Portfolio, TradeRecord};
use crate::engine::execution::ExecutionConfig;
use crate::engine::order_book::OrderBook;
use crate::engine::stickiness::StickinessMetrics;
use crate::fingerprint::TradingMode;
use std::collections::HashMap;

/// Configuration for a single backtest run.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub initial_capital: f64,
    /// Computed from max indicator lookback. No signals/orders during warmup.
    pub warmup_bars: usize,
    pub trading_mode: TradingMode,
    /// Execution engine configuration (path policy, gap policy, cost model, liquidity).
    pub execution_config: ExecutionConfig,
    /// Per-symbol instrument metadata (tick size, lot size, etc.).
    pub instruments: HashMap<String, Instrument>,
    /// Fraction of equity to allocate per position (default 1.0 = 100%).
    pub position_size_pct: f64,
}

impl EngineConfig {
    pub fn new(initial_capital: f64, warmup_bars: usize) -> Self {
        Self {
            initial_capital,
            warmup_bars,
            trading_mode: TradingMode::LongOnly,
            execution_config: ExecutionConfig::frictionless(),
            instruments: HashMap::new(),
            position_size_pct: 1.0,
        }
    }

    /// Create a config with an explicit execution preset.
    pub fn with_execution(
        initial_capital: f64,
        warmup_bars: usize,
        execution_config: ExecutionConfig,
    ) -> Self {
        Self {
            initial_capital,
            warmup_bars,
            trading_mode: TradingMode::LongOnly,
            execution_config,
            instruments: HashMap::new(),
            position_size_pct: 1.0,
        }
    }
}

/// Mutable state that evolves bar-by-bar during the engine loop.
pub struct EngineState {
    pub portfolio: Portfolio,
    pub order_book: OrderBook,
    pub id_gen: IdGen,
    pub bar_index: usize,
    pub warmup_complete: bool,
    /// Last valid close price per symbol (for void bar equity carry-forward).
    pub last_valid_close: HashMap<String, f64>,
    /// Count of void bars per symbol (for data quality tracking).
    pub void_bar_counts: HashMap<String, usize>,
    /// Total bars processed per symbol.
    pub total_bar_counts: HashMap<String, usize>,
    /// Active stop order ID per symbol, for PM cancel/replace.
    pub stop_order_ids: HashMap<String, OrderId>,
    /// Total PM on_bar calls made (for stickiness diagnostics).
    pub pm_calls_total: usize,
    /// PM calls that returned AdjustStop or ForceExit (non-Hold).
    pub pm_calls_active: usize,
    /// Total signals fired during the run.
    pub signal_count: usize,
    /// Records of all signal filter evaluations (for diagnostics).
    pub signal_evaluations: Vec<SignalEvaluation>,
    /// Maps symbol -> last entry signal (for reference by downstream components).
    pub entry_signals: HashMap<String, SignalEvent>,
}

impl EngineState {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            portfolio: Portfolio::new(initial_capital),
            order_book: OrderBook::new(),
            id_gen: IdGen::default(),
            bar_index: 0,
            warmup_complete: false,
            last_valid_close: HashMap::new(),
            void_bar_counts: HashMap::new(),
            total_bar_counts: HashMap::new(),
            stop_order_ids: HashMap::new(),
            pm_calls_total: 0,
            pm_calls_active: 0,
            signal_count: 0,
            signal_evaluations: Vec::new(),
            entry_signals: HashMap::new(),
        }
    }

    /// Verify the equity accounting identity: equity == cash + sum(position market values).
    ///
    /// Returns the current equity. Panics in debug mode if the identity is violated.
    pub fn verify_equity(&self, prices: &HashMap<String, f64>) -> f64 {
        let equity = self.portfolio.equity(prices);

        #[cfg(debug_assertions)]
        {
            let position_value: f64 = self
                .portfolio
                .positions
                .iter()
                .map(|(sym, pos)| {
                    let price = prices.get(sym).copied().unwrap_or(pos.avg_entry_price);
                    pos.market_value(price)
                })
                .sum();
            let expected = self.portfolio.cash + position_value;
            assert!(
                (equity - expected).abs() < 1e-10,
                "equity accounting violated: equity={equity}, cash={} + positions={position_value} = {expected}",
                self.portfolio.cash
            );
        }

        equity
    }

    /// Compute void bar rates per symbol. Returns rate as fraction (0.0 to 1.0).
    pub fn void_bar_rates(&self) -> HashMap<String, f64> {
        self.total_bar_counts
            .iter()
            .map(|(sym, &total)| {
                let void_count = self.void_bar_counts.get(sym).copied().unwrap_or(0);
                let rate = if total > 0 {
                    void_count as f64 / total as f64
                } else {
                    0.0
                };
                (sym.clone(), rate)
            })
            .collect()
    }

    /// Get current position sides for all open positions.
    pub fn position_sides(&self) -> HashMap<String, crate::domain::position::PositionSide> {
        self.portfolio
            .positions
            .iter()
            .filter(|(_, pos)| !pos.is_flat())
            .map(|(sym, pos)| (sym.clone(), pos.side))
            .collect()
    }
}

/// Result of a complete backtest run.
#[derive(Debug)]
pub struct RunResult {
    /// Equity value at each bar close.
    pub equity_curve: Vec<f64>,
    /// All fills generated during the run.
    pub fills: Vec<Fill>,
    /// Completed round-trip trades.
    pub trades: Vec<TradeRecord>,
    /// Final equity value.
    pub final_equity: f64,
    /// Total number of bars processed.
    pub bar_count: usize,
    /// Number of warmup bars skipped.
    pub warmup_bars: usize,
    /// Void bar rate per symbol (fraction 0.0 to 1.0).
    pub void_bar_rates: HashMap<String, f64>,
    /// Data quality warnings (e.g., "SPY: 12% void bars exceeds 10% threshold").
    pub data_quality_warnings: Vec<String>,
    /// Stickiness diagnostics (None if no trades completed).
    pub stickiness: Option<StickinessMetrics>,
    /// Total signals fired during the run.
    pub signal_count: usize,
    /// All signal filter evaluations (for diagnostics).
    pub signal_evaluations: Vec<SignalEvaluation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_config_defaults() {
        let config = EngineConfig::new(100_000.0, 20);
        assert_eq!(config.initial_capital, 100_000.0);
        assert_eq!(config.warmup_bars, 20);
        assert_eq!(config.trading_mode, TradingMode::LongOnly);
    }

    #[test]
    fn engine_state_initial() {
        let state = EngineState::new(100_000.0);
        assert_eq!(state.portfolio.cash, 100_000.0);
        assert_eq!(state.bar_index, 0);
        assert!(!state.warmup_complete);
        assert!(state.last_valid_close.is_empty());
    }

    #[test]
    fn verify_equity_flat_portfolio() {
        let state = EngineState::new(100_000.0);
        let prices = HashMap::new();
        let equity = state.verify_equity(&prices);
        assert_eq!(equity, 100_000.0);
    }

    #[test]
    fn void_bar_rates_calculation() {
        let mut state = EngineState::new(100_000.0);
        state.total_bar_counts.insert("SPY".into(), 100);
        state.void_bar_counts.insert("SPY".into(), 15);
        state.total_bar_counts.insert("QQQ".into(), 100);
        // QQQ has no void bars

        let rates = state.void_bar_rates();
        assert!((rates["SPY"] - 0.15).abs() < 1e-10);
        assert!((rates["QQQ"] - 0.0).abs() < 1e-10);
    }
}
