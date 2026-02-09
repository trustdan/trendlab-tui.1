//! Golden test — exact expected values from a known strategy on frozen SPY 2024 data.
//!
//! This test must break if anyone changes the engine's behavior. It locks:
//! - Exact trade count
//! - Exact final equity (to penny precision)
//! - Exact equity curve values at specific bars
//! - Exact trade list (entry/exit bars, PnL)
//! - Exact performance metrics
//!
//! Strategy: MomentumRoc (period=12, threshold=0%) with time_decay PM,
//!           next_bar_open execution, volatility_filter.
//!
//! If this test fails, it means the engine produces different results than
//! the locked golden values. This is intentional — investigate before updating.

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
        "trendlab_golden_{}_{id}",
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

fn format_params(section: &str, params: &std::collections::BTreeMap<String, f64>) -> String {
    if params.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = params.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    format!("[{section}.params]\n{}", pairs.join("\n"))
}

fn run_golden_strategy() -> trendlab_runner::BacktestResult {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);
    let opts = load_opts();

    let strategy_config = StrategyPreset::MomentumRoc.to_config();
    let toml_str = format!(
        r#"[backtest]
symbol = "SPY"
start_date = "2024-01-02"
end_date = "2024-12-31"
initial_capital = 100000.0
trading_mode = "long_short"
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

    let config = BacktestConfig::from_toml(&toml_str).unwrap();
    let result = run_single_backtest(&config, &cache, None, &opts).unwrap();

    let _ = std::fs::remove_dir_all(&cache_dir);
    result
}

// ── Tolerance for floating-point comparisons ─────────────────────────

const EPS: f64 = 1e-4; // penny-level for equity, tight for ratios

fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

// ── Locked golden values ─────────────────────────────────────────────
//
// These values were generated from the frozen SPY 2024 fixture using the
// MomentumRoc preset (period=12, threshold=0%, time_decay PM,
// next_bar_open execution, volatility_filter) on 2026-02-09.
//
// If any assertion fails, the engine's behavior has changed.
// Investigate before updating.

#[test]
fn golden_trade_count() {
    let result = run_golden_strategy();
    assert_eq!(result.trades.len(), 8, "trade count changed");
}

#[test]
fn golden_signal_count() {
    let result = run_golden_strategy();
    assert_eq!(result.signal_count, 9, "signal count changed");
}

#[test]
fn golden_bar_count_and_warmup() {
    let result = run_golden_strategy();
    assert_eq!(result.bar_count, 252, "bar count changed");
    assert_eq!(result.warmup_bars, 14, "warmup bars changed");
}

#[test]
fn golden_final_equity() {
    let result = run_golden_strategy();
    let final_eq = *result.equity_curve.last().unwrap();
    assert!(
        approx_eq(final_eq, 283396.707492, EPS),
        "final equity changed: got {final_eq:.6}, expected 283396.707492"
    );
}

#[test]
fn golden_equity_curve_checkpoints() {
    let result = run_golden_strategy();

    let checks: &[(usize, f64)] = &[
        (0, 100000.0),
        (10, 100000.0),
        (50, 105437.580408),
        (100, 107480.027785),
        (150, 311739.331062),
        (200, 92447.876096),
        (251, 283396.707492),
    ];

    for &(bar, expected) in checks {
        let actual = result.equity_curve[bar];
        assert!(
            approx_eq(actual, expected, EPS),
            "equity at bar {bar} changed: got {actual:.6}, expected {expected:.6}"
        );
    }
}

#[test]
fn golden_performance_metrics() {
    let result = run_golden_strategy();
    let m = &result.metrics;

    assert!(
        approx_eq(m.total_return, 1.8339670749, EPS),
        "total_return changed: {:.10}",
        m.total_return
    );
    assert!(
        approx_eq(m.sharpe, 1.4550805886, 0.001),
        "sharpe changed: {:.10}",
        m.sharpe
    );
    assert!(
        approx_eq(m.sortino, 5.1240484664, 0.01),
        "sortino changed: {:.10}",
        m.sortino
    );
    assert!(
        approx_eq(m.max_drawdown, -0.7185625354, 0.001),
        "max_drawdown changed: {:.10}",
        m.max_drawdown
    );
    assert!(
        approx_eq(m.win_rate, 0.625, EPS),
        "win_rate changed: {:.10}",
        m.win_rate
    );
    assert!(
        approx_eq(m.profit_factor, 0.7107852113, 0.001),
        "profit_factor changed: {:.10}",
        m.profit_factor
    );
}

#[test]
fn golden_trade_details() {
    let result = run_golden_strategy();
    assert_eq!(result.trades.len(), 8);

    // Trade 0: Long entry_bar=15, exit_bar=68
    assert_eq!(result.trades[0].entry_bar, 15);
    assert_eq!(result.trades[0].exit_bar, 68);
    assert!(approx_eq(result.trades[0].entry_price, 476.28, 0.01));
    assert!(approx_eq(result.trades[0].exit_price, 501.64, 0.01));

    // Trade 3: Long entry_bar=137, exit_bar=148 (a loser)
    assert_eq!(result.trades[3].entry_bar, 137);
    assert_eq!(result.trades[3].exit_bar, 148);
    assert!(result.trades[3].net_pnl < 0.0, "trade 3 should be a loser");

    // Trade 7: Last trade, Long entry_bar=222, exit_bar=243
    assert_eq!(result.trades[7].entry_bar, 222);
    assert_eq!(result.trades[7].exit_bar, 243);
    assert!(result.trades[7].net_pnl > 0.0, "trade 7 should be a winner");

    // Signal traceability: all trades should have component names
    for (i, t) in result.trades.iter().enumerate() {
        assert!(
            t.signal_type.is_some(),
            "trade {i} missing signal_type"
        );
        assert!(
            t.pm_type.is_some(),
            "trade {i} missing pm_type"
        );
        assert!(
            t.execution_model.is_some(),
            "trade {i} missing execution_model"
        );
        assert!(
            t.filter_type.is_some(),
            "trade {i} missing filter_type"
        );
    }
}
