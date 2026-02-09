//! Backtest runner — wires together composition, engine, and metrics.
//!
//! Three entry points:
//! - `run_single_backtest()`: loads data from cache, then runs. Used by CLI.
//! - `run_backtest_from_data()`: takes pre-loaded data + execution preset. Used by YOLO mode.
//! - `run_backtest_with_exec_config()`: takes pre-loaded data + explicit ExecutionConfig.
//!   Used by execution Monte Carlo.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use trendlab_core::components::composition::build_composition;
use trendlab_core::components::execution::ExecutionPreset;
use trendlab_core::components::factory::FactoryError;
use trendlab_core::data::align::AlignedData;
use trendlab_core::data::cache::ParquetCache;
use trendlab_core::data::provider::DataProvider;
use trendlab_core::domain::TradeRecord;
use trendlab_core::engine::stickiness::StickinessMetrics;
use trendlab_core::engine::{run_backtest, EngineConfig, ExecutionConfig};
use trendlab_core::fingerprint::{StrategyConfig, TradingMode};

use crate::config::{BacktestConfig, ConfigError};
use crate::data_loader::{load_bars, LoadError, LoadOptions};
use crate::metrics::PerformanceMetrics;

/// Errors from the runner.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("data error: {0}")]
    Data(#[from] LoadError),
    #[error("composition error: {0}")]
    Composition(#[from] FactoryError),
    #[error("symbol '{0}' not found in loaded data")]
    SymbolNotFound(String),
}

/// Current schema version for persisted artifacts.
pub const SCHEMA_VERSION: u32 = 1;

/// Complete result of a single backtest run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    /// Schema version for forward-compatible deserialization.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub metrics: PerformanceMetrics,
    pub trades: Vec<TradeRecord>,
    pub equity_curve: Vec<f64>,
    pub config: StrategyConfig,
    pub symbol: String,
    pub start_date: String,
    pub end_date: String,
    pub initial_capital: f64,
    pub dataset_hash: String,
    pub has_synthetic: bool,
    pub signal_count: usize,
    pub bar_count: usize,
    pub warmup_bars: usize,
    pub void_bar_rates: HashMap<String, f64>,
    pub data_quality_warnings: Vec<String>,
    /// Stickiness diagnostics from the position manager (None if zero trades).
    pub stickiness: Option<StickinessMetrics>,
}

/// Default schema version for serde deserialization of older JSON without the field.
fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// Run a single backtest from a BacktestConfig (loads data from cache).
///
/// This is the high-level entry point used by the CLI. For pre-loaded data
/// (YOLO mode), use `run_backtest_from_data()` instead.
pub fn run_single_backtest(
    config: &BacktestConfig,
    cache: &ParquetCache,
    provider: Option<&dyn DataProvider>,
    opts: &LoadOptions,
) -> Result<BacktestResult, RunError> {
    let symbol = &config.backtest.symbol;
    let loaded = load_bars(&[symbol.as_str()], cache, provider, None, opts)?;
    let strategy_config = config.to_strategy_config();
    let preset = decode_execution_preset(&config.execution_model.params);

    run_backtest_from_data(
        &strategy_config,
        &loaded.aligned,
        symbol,
        config.trading_mode(),
        config.backtest.initial_capital,
        config.backtest.position_size_pct,
        preset,
        &loaded.dataset_hash,
        loaded.has_synthetic,
    )
}

/// Run a backtest with pre-loaded data — no I/O.
///
/// Used by YOLO mode to avoid re-reading Parquet on every iteration.
/// The `aligned` data may contain multiple symbols; only `symbol` is used.
#[allow(clippy::too_many_arguments)]
pub fn run_backtest_from_data(
    strategy_config: &StrategyConfig,
    aligned: &AlignedData,
    symbol: &str,
    trading_mode: TradingMode,
    initial_capital: f64,
    position_size_pct: f64,
    execution_preset: ExecutionPreset,
    dataset_hash: &str,
    has_synthetic: bool,
) -> Result<BacktestResult, RunError> {
    run_backtest_with_exec_config(
        strategy_config,
        aligned,
        symbol,
        trading_mode,
        initial_capital,
        position_size_pct,
        ExecutionConfig::from_preset(execution_preset),
        dataset_hash,
        has_synthetic,
    )
}

/// Run a backtest with pre-loaded data and an explicit ExecutionConfig.
///
/// Used by execution Monte Carlo to test varying slippage/commission parameters.
/// Same as `run_backtest_from_data` but takes `ExecutionConfig` directly.
#[allow(clippy::too_many_arguments)]
pub fn run_backtest_with_exec_config(
    strategy_config: &StrategyConfig,
    aligned: &AlignedData,
    symbol: &str,
    trading_mode: TradingMode,
    initial_capital: f64,
    position_size_pct: f64,
    exec_config: ExecutionConfig,
    dataset_hash: &str,
    has_synthetic: bool,
) -> Result<BacktestResult, RunError> {
    // Verify symbol exists in aligned data
    if !aligned.bars.contains_key(symbol) {
        return Err(RunError::SymbolNotFound(symbol.to_string()));
    }

    // Extract single-symbol AlignedData for the engine
    let single_aligned = extract_single_symbol(aligned, symbol);

    // Build composition from strategy config
    let composition = build_composition(strategy_config, trading_mode)?;

    // Configure engine
    let mut engine_config = EngineConfig::with_execution(
        initial_capital,
        0, // warmup computed from indicator lookbacks
        exec_config,
    );
    engine_config.trading_mode = trading_mode;
    engine_config.position_size_pct = position_size_pct;

    // Run the bar-by-bar event loop
    let result = run_backtest(
        &single_aligned,
        &composition.indicators,
        &engine_config,
        composition.signal.as_ref(),
        composition.filter.as_ref(),
        composition.execution.as_ref(),
        composition.pm.as_ref(),
    );

    // Compute metrics
    let metrics =
        PerformanceMetrics::compute(&result.equity_curve, &result.trades, initial_capital);

    // Annotate trades with component names
    let mut trades = result.trades;
    for trade in &mut trades {
        trade.signal_type = Some(composition.signal.name().to_string());
        trade.pm_type = Some(composition.pm.name().to_string());
        trade.execution_model = Some(composition.execution.name().to_string());
        trade.filter_type = Some(composition.filter.name().to_string());
    }

    // Derive date strings from aligned data
    let start_date = single_aligned
        .dates
        .first()
        .map(|d| d.to_string())
        .unwrap_or_default();
    let end_date = single_aligned
        .dates
        .last()
        .map(|d| d.to_string())
        .unwrap_or_default();

    Ok(BacktestResult {
        schema_version: SCHEMA_VERSION,
        metrics,
        trades,
        equity_curve: result.equity_curve,
        config: strategy_config.clone(),
        symbol: symbol.to_string(),
        start_date,
        end_date,
        initial_capital,
        dataset_hash: dataset_hash.to_string(),
        has_synthetic,
        signal_count: result.signal_count,
        bar_count: result.bar_count,
        warmup_bars: result.warmup_bars,
        void_bar_rates: result.void_bar_rates,
        data_quality_warnings: result.data_quality_warnings,
        stickiness: result.stickiness,
    })
}

/// Extract a single symbol's data from a multi-symbol AlignedData.
fn extract_single_symbol(aligned: &AlignedData, symbol: &str) -> AlignedData {
    let bars = aligned.bars.get(symbol).cloned().unwrap_or_default();

    let mut bar_map = HashMap::new();
    bar_map.insert(symbol.to_string(), bars);

    AlignedData {
        dates: aligned.dates.clone(),
        bars: bar_map,
        symbols: vec![symbol.to_string()],
    }
}

/// Decode the execution preset from config params.
pub fn decode_execution_preset(
    params: &std::collections::BTreeMap<String, f64>,
) -> ExecutionPreset {
    match params.get("preset").copied().unwrap_or(1.0) as u8 {
        0 => ExecutionPreset::Frictionless,
        2 => ExecutionPreset::Hostile,
        3 => ExecutionPreset::Optimistic,
        _ => ExecutionPreset::Realistic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_preset_realistic_default() {
        let params = std::collections::BTreeMap::new();
        assert_eq!(
            decode_execution_preset(&params) as u8,
            ExecutionPreset::Realistic as u8
        );
    }

    #[test]
    fn decode_preset_frictionless() {
        let mut params = std::collections::BTreeMap::new();
        params.insert("preset".into(), 0.0);
        assert_eq!(
            decode_execution_preset(&params) as u8,
            ExecutionPreset::Frictionless as u8
        );
    }

    #[test]
    fn decode_preset_hostile() {
        let mut params = std::collections::BTreeMap::new();
        params.insert("preset".into(), 2.0);
        assert_eq!(
            decode_execution_preset(&params) as u8,
            ExecutionPreset::Hostile as u8
        );
    }
}
