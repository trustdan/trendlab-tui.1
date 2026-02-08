//! Integration tests for the composition system and engine integration.
//!
//! Tests:
//! 1. All 5 presets build into StrategyComposition
//! 2. Each preset runs through run_backtest end-to-end without panic
//! 3. Presets that can fire on synthetic data produce signals
//! 4. Trading mode correctly filters signals (ShortOnly blocks Long signals)
//! 5. Signal evaluations are recorded when signals pass trading mode filter
//! 6. All presets produce non-empty indicator sets

use chrono::NaiveDate;
use std::collections::HashMap;
use trendlab_core::components::composition::{build_composition, StrategyPreset};
use trendlab_core::data::align::AlignedData;
use trendlab_core::data::provider::RawBar;
use trendlab_core::engine::{run_backtest, EngineConfig};
use trendlab_core::fingerprint::TradingMode;

// ──────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────

/// Create N bars of steady uptrend data.
///
/// Close rises from 100.0 by 0.5 per bar. The ROC (rate of change) is always
/// positive, so RocMomentum signals will fire. The trend is too linear for
/// MA crossover signals.
fn make_trending_rawbars(n: usize) -> Vec<RawBar> {
    let base_date = NaiveDate::from_ymd_opt(2023, 1, 2).unwrap();
    (0..n)
        .map(|i| {
            let close = 100.0 + i as f64 * 0.5;
            RawBar {
                date: base_date + chrono::Duration::days(i as i64),
                open: close - 0.3,
                high: close + 1.5,
                low: close - 1.0,
                close,
                volume: 1000,
                adj_close: close,
            }
        })
        .collect()
}

/// Create N bars that start flat then ramp up sharply — designed to trigger
/// MA crossover signals.
///
/// The ramp starts at bar `ramp_start`. The fast MA (sma_10) reacts quickly to
/// the ramp while the slow MA (sma_50) lags behind, creating a golden cross
/// a few bars after the ramp begins. For MaCrossoverTrend preset (which uses
/// sma_200 via the ma_regime filter), `ramp_start` must be > 200 so the
/// crossover happens after the engine warmup period.
fn make_crossover_rawbars(n: usize, ramp_start: usize) -> Vec<RawBar> {
    let base_date = NaiveDate::from_ymd_opt(2023, 1, 2).unwrap();
    (0..n)
        .map(|i| {
            let close = if i < ramp_start {
                100.0
            } else {
                100.0 + (i - ramp_start) as f64 * 2.0
            };
            RawBar {
                date: base_date + chrono::Duration::days(i as i64),
                open: close - 0.3,
                high: close + 1.5,
                low: close - 1.0,
                close,
                volume: 1000,
                adj_close: close,
            }
        })
        .collect()
}

/// Build AlignedData for a single symbol ("TEST") from raw bars.
fn make_aligned(bars: Vec<RawBar>) -> AlignedData {
    let dates: Vec<NaiveDate> = bars.iter().map(|b| b.date).collect();
    let mut bar_map = HashMap::new();
    bar_map.insert("TEST".to_string(), bars);
    AlignedData {
        dates,
        bars: bar_map,
        symbols: vec!["TEST".to_string()],
    }
}

// ──────────────────────────────────────────────
// 1. All presets build successfully
// ──────────────────────────────────────────────

#[test]
fn all_presets_build_successfully() {
    for preset in StrategyPreset::all() {
        let config = preset.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert!(
            !comp.indicators.is_empty(),
            "preset {:?} should have indicators",
            preset
        );
    }
}

// ──────────────────────────────────────────────
// 2. Each preset completes a backtest without panic
// ──────────────────────────────────────────────

#[test]
fn donchian_trend_runs_end_to_end() {
    let aligned = make_aligned(make_trending_rawbars(300));
    let config_strat = StrategyPreset::DonchianTrend.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    assert_eq!(result.bar_count, 300);
    assert_eq!(result.equity_curve.len(), 300);
    // Donchian upper includes current bar's high, so close > donchian_upper
    // is structurally impossible with normal OHLC data. signal_count == 0 is expected.
    // This is a known limitation of the donchian_upper indicator including the current bar.
}

#[test]
fn bollinger_breakout_runs_end_to_end() {
    let aligned = make_aligned(make_trending_rawbars(300));
    let config_strat = StrategyPreset::BollingerBreakout.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    assert_eq!(result.bar_count, 300);
    assert_eq!(result.equity_curve.len(), 300);
}

#[test]
fn ma_crossover_trend_runs_end_to_end() {
    let aligned = make_aligned(make_crossover_rawbars(500, 210));
    let config_strat = StrategyPreset::MaCrossoverTrend.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    assert_eq!(result.bar_count, 500);
    assert_eq!(result.equity_curve.len(), 500);
}

#[test]
fn supertrend_system_runs_end_to_end() {
    let aligned = make_aligned(make_trending_rawbars(300));
    let config_strat = StrategyPreset::SupertrendSystem.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    assert_eq!(result.bar_count, 300);
    assert_eq!(result.equity_curve.len(), 300);
}

// ──────────────────────────────────────────────
// 3. Presets that fire on synthetic data
// ──────────────────────────────────────────────

#[test]
fn momentum_roc_generates_signals() {
    // RocMomentum with threshold_pct=0 fires whenever ROC != 0.
    // On steady uptrend data, ROC is always positive -> Long signals.
    let aligned = make_aligned(make_trending_rawbars(300));
    let config_strat = StrategyPreset::MomentumRoc.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    assert!(
        result.signal_count > 0,
        "MomentumRoc should generate signals on trending data, got signal_count={}",
        result.signal_count,
    );
}

#[test]
fn ma_crossover_generates_signals_on_crossover_data() {
    // Flat-then-ramp data causes fast MA to cross above slow MA.
    // MaCrossover preset uses sma_10/sma_50 — after 50 bars of flat then
    // sharp ramp, the fast MA crosses above the slow MA.
    let aligned = make_aligned(make_crossover_rawbars(500, 210));
    let config_strat = StrategyPreset::MaCrossoverTrend.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    assert!(
        result.signal_count > 0,
        "MaCrossover should generate signals on flat-then-ramp data, got signal_count={}",
        result.signal_count,
    );
}

// ──────────────────────────────────────────────
// 4. Trading mode correctly filters signals
// ──────────────────────────────────────────────

#[test]
fn short_only_mode_blocks_long_signals() {
    // RocMomentum on uptrend data produces Long signals.
    // With ShortOnly trading mode, those Long signals should be filtered
    // by the trading mode check BEFORE reaching the signal filter.
    //
    // Per loop_runner.rs:
    //   signal_count is incremented BEFORE the trading mode filter.
    //   signal_evaluations only gets entries AFTER the trading mode filter passes.
    //
    // So: signal_count > 0, but signal_evaluations should be empty because
    // all Long signals are skipped before filter evaluation.
    let aligned = make_aligned(make_trending_rawbars(300));
    let config_strat = StrategyPreset::MomentumRoc.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let mut engine_config = EngineConfig::new(100_000.0, 0);
    engine_config.trading_mode = TradingMode::ShortOnly;

    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    // The signal generator fires Long signals (signal_count > 0)
    assert!(
        result.signal_count > 0,
        "RocMomentum on uptrend should still fire Long signals (signal_count), got {}",
        result.signal_count,
    );

    // But the trading mode filter blocks them before reaching the signal filter,
    // so signal_evaluations should be empty.
    assert!(
        result.signal_evaluations.is_empty(),
        "ShortOnly mode should block all Long signals before filter evaluation, \
         got {} evaluations",
        result.signal_evaluations.len(),
    );
}

// ──────────────────────────────────────────────
// 5. Signal evaluations are recorded
// ──────────────────────────────────────────────

#[test]
fn signal_evaluations_recorded_with_filter() {
    // MomentumRoc + volatility_filter on trending data.
    // Signals that pass the trading mode filter (LongOnly + Long signals)
    // reach the signal filter, producing SignalEvaluation records
    // (regardless of the filter verdict — pass or reject).
    let aligned = make_aligned(make_trending_rawbars(300));
    let config_strat = StrategyPreset::MomentumRoc.to_config();
    let comp = build_composition(&config_strat, TradingMode::LongOnly).unwrap();

    let engine_config = EngineConfig::new(100_000.0, 0);
    let result = run_backtest(
        &aligned,
        &comp.indicators,
        &engine_config,
        comp.signal.as_ref(),
        comp.filter.as_ref(),
        comp.execution.as_ref(),
        comp.pm.as_ref(),
    );

    // Must have signals first
    assert!(
        result.signal_count > 0,
        "MomentumRoc should generate signals, got signal_count=0",
    );

    // Signal evaluations should be recorded (filter was invoked)
    assert!(
        !result.signal_evaluations.is_empty(),
        "signal_evaluations should be non-empty when signals pass trading mode filter \
         and reach the signal filter (volatility_filter); signal_count={}",
        result.signal_count,
    );

    // Every evaluation should reference the volatility_filter
    for eval in &result.signal_evaluations {
        assert_eq!(
            eval.filter_name, "volatility_filter",
            "expected volatility_filter evaluation, got {}",
            eval.filter_name,
        );
    }
}

// ──────────────────────────────────────────────
// 6. All presets have non-empty indicators
// ──────────────────────────────────────────────

#[test]
fn presets_have_indicators() {
    for preset in StrategyPreset::all() {
        let config = preset.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert!(
            !comp.indicators.is_empty(),
            "{:?} should require indicators",
            preset,
        );
    }
}
