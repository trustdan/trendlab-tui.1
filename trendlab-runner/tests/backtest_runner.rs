//! Integration tests for the runner: real strategies on real SPY data.
//!
//! Uses the frozen SPY 2024 fixture to run all five presets
//! and verify trade extraction, metrics, and end-to-end correctness.

use chrono::NaiveDate;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use trendlab_core::components::composition::StrategyPreset;
use trendlab_core::data::cache::ParquetCache;
use trendlab_runner::config::BacktestConfig;
use trendlab_runner::data_loader::LoadOptions;
use trendlab_runner::runner::run_single_backtest;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn core_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("trendlab-core/tests/fixtures")
}

fn setup_fixture_cache() -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir = std::env::temp_dir().join(format!(
        "trendlab_runner_backtest_{}_{id}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&cache_dir);

    let sym_dir = cache_dir.join("symbol=SPY");
    std::fs::create_dir_all(&sym_dir).unwrap();
    std::fs::copy(
        core_fixture_dir().join("spy_2024.parquet"),
        sym_dir.join("2024.parquet"),
    )
    .unwrap();

    let meta = r#"{"symbol":"SPY","start_date":"2024-01-02","end_date":"2024-12-31","bar_count":252,"data_hash":"fixture","source":"fixture","cached_at":"2024-01-01T00:00:00"}"#;
    std::fs::write(sym_dir.join("meta.json"), meta).unwrap();

    cache_dir
}

fn load_opts() -> LoadOptions {
    LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    }
}

fn config_from_preset(preset: StrategyPreset) -> BacktestConfig {
    let strategy_config = preset.to_config();
    let toml_str = format!(
        r#"[backtest]
symbol = "SPY"
start_date = "2024-01-02"
end_date = "2024-12-31"
initial_capital = 100000.0
trading_mode = "long_only"
position_size_pct = 1.0

[signal]
type = "{sig_type}"
{sig_params}

[position_manager]
type = "{pm_type}"
{pm_params}

[execution_model]
type = "{exec_type}"
{exec_params}

[signal_filter]
type = "{filter_type}"
{filter_params}
"#,
        sig_type = strategy_config.signal.component_type,
        sig_params = format_params("signal", &strategy_config.signal.params),
        pm_type = strategy_config.position_manager.component_type,
        pm_params = format_params("position_manager", &strategy_config.position_manager.params),
        exec_type = strategy_config.execution_model.component_type,
        exec_params = format_params("execution_model", &strategy_config.execution_model.params),
        filter_type = strategy_config.signal_filter.component_type,
        filter_params = format_params("signal_filter", &strategy_config.signal_filter.params),
    );
    BacktestConfig::from_toml(&toml_str).unwrap()
}

fn format_params(section: &str, params: &std::collections::BTreeMap<String, f64>) -> String {
    if params.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = params.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    format!("[{section}.params]\n{}", pairs.join("\n"))
}

// ── Per-preset tests ─────────────────────────────────────────────

fn run_preset_and_verify(preset: StrategyPreset, name: &str) {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);
    let config = config_from_preset(preset);
    let opts = load_opts();

    let result = run_single_backtest(&config, &cache, None, &opts)
        .unwrap_or_else(|e| panic!("{name} failed: {e}"));

    // Equity curve length = bar count
    assert_eq!(
        result.equity_curve.len(),
        result.bar_count,
        "{name}: equity curve length mismatch"
    );

    // All metrics are finite
    assert!(
        result.metrics.total_return.is_finite(),
        "{name}: total_return not finite"
    );
    assert!(result.metrics.cagr.is_finite(), "{name}: cagr not finite");
    assert!(
        result.metrics.sharpe.is_finite(),
        "{name}: sharpe not finite"
    );
    assert!(
        result.metrics.sortino.is_finite(),
        "{name}: sortino not finite"
    );
    assert!(
        result.metrics.calmar.is_finite(),
        "{name}: calmar not finite"
    );
    assert!(
        result.metrics.max_drawdown.is_finite(),
        "{name}: max_drawdown not finite"
    );
    assert!(
        result.metrics.win_rate.is_finite(),
        "{name}: win_rate not finite"
    );
    assert!(
        result.metrics.profit_factor.is_finite(),
        "{name}: profit_factor not finite"
    );
    assert!(
        result.metrics.turnover.is_finite(),
        "{name}: turnover not finite"
    );
    assert!(
        result.metrics.avg_losing_streak.is_finite(),
        "{name}: avg_losing_streak not finite"
    );

    // Metrics in reasonable ranges
    assert!(
        result.metrics.sharpe > -5.0 && result.metrics.sharpe < 5.0,
        "{name}: sharpe out of range: {}",
        result.metrics.sharpe
    );
    assert!(
        result.metrics.max_drawdown <= 0.0 && result.metrics.max_drawdown > -1.0,
        "{name}: max_drawdown out of range: {}",
        result.metrics.max_drawdown
    );
    assert!(
        result.metrics.win_rate >= 0.0 && result.metrics.win_rate <= 1.0,
        "{name}: win_rate out of range: {}",
        result.metrics.win_rate
    );

    // Signal trace populated on trades
    if !result.trades.is_empty() {
        let has_signal_type = result.trades.iter().any(|t| t.signal_type.is_some());
        assert!(has_signal_type, "{name}: no trades have signal_type set");
    }

    println!(
        "{name}: {} trades, Sharpe={:.3}, CAGR={:.2}%, MaxDD={:.2}%",
        result.trades.len(),
        result.metrics.sharpe,
        result.metrics.cagr * 100.0,
        result.metrics.max_drawdown * 100.0
    );

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn preset_donchian_trend_on_spy() {
    run_preset_and_verify(StrategyPreset::DonchianTrend, "DonchianTrend");
}

#[test]
fn preset_bollinger_breakout_on_spy() {
    run_preset_and_verify(StrategyPreset::BollingerBreakout, "BollingerBreakout");
}

#[test]
fn preset_ma_crossover_on_spy() {
    run_preset_and_verify(StrategyPreset::MaCrossoverTrend, "MaCrossoverTrend");
}

#[test]
fn preset_momentum_roc_on_spy() {
    run_preset_and_verify(StrategyPreset::MomentumRoc, "MomentumRoc");
}

#[test]
fn preset_supertrend_on_spy() {
    run_preset_and_verify(StrategyPreset::SupertrendSystem, "SupertrendSystem");
}

// ── Cross-preset distinctness ────────────────────────────────────

#[test]
fn presets_produce_distinct_results() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);
    let opts = load_opts();

    let mut trade_counts: Vec<(String, usize)> = Vec::new();

    for preset in StrategyPreset::all() {
        let config = config_from_preset(*preset);
        let result = run_single_backtest(&config, &cache, None, &opts).unwrap();
        trade_counts.push((format!("{:?}", preset), result.trades.len()));
    }

    // At least 3 of 5 presets should produce different trade counts
    let unique_counts: std::collections::HashSet<usize> =
        trade_counts.iter().map(|(_, c)| *c).collect();
    assert!(
        unique_counts.len() >= 3,
        "Expected at least 3 distinct trade counts, got {}: {:?}",
        unique_counts.len(),
        trade_counts
    );

    let _ = std::fs::remove_dir_all(&cache_dir);
}

// ── Zero-signal edge case ────────────────────────────────────────

#[test]
fn null_signal_zero_trades() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);
    let opts = load_opts();

    // Use a signal that never fires: donchian with extremely long lookback
    // (longer than the data, so no breakout possible)
    let toml_str = r#"
[backtest]
symbol = "SPY"
start_date = "2024-01-02"
end_date = "2024-12-31"
initial_capital = 100000.0

[signal]
type = "donchian_breakout"
[signal.params]
entry_lookback = 300.0

[position_manager]
type = "no_op"

[execution_model]
type = "next_bar_open"
"#;

    let config = BacktestConfig::from_toml(toml_str).unwrap();
    let result = run_single_backtest(&config, &cache, None, &opts).unwrap();

    // With lookback 300 on 252 bars of data, no breakout signal should fire
    assert_eq!(result.trades.len(), 0, "expected zero trades");
    assert_eq!(result.metrics.trade_count, 0);
    assert!((result.metrics.total_return - 0.0).abs() < 1e-10);
    assert!((result.metrics.sharpe - 0.0).abs() < 1e-10);

    let _ = std::fs::remove_dir_all(&cache_dir);
}

// ── BacktestResult serialization ─────────────────────────────────

#[test]
fn backtest_result_serializes_to_json() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);
    let opts = load_opts();
    let config = config_from_preset(StrategyPreset::DonchianTrend);

    let result = run_single_backtest(&config, &cache, None, &opts).unwrap();

    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.is_empty());

    // Verify it's valid JSON by parsing back
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(value.get("metrics").is_some());
    assert!(value.get("symbol").is_some());
    assert!(value.get("equity_curve").is_some());

    let _ = std::fs::remove_dir_all(&cache_dir);
}
