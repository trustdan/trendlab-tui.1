//! Integration tests for the engine event loop.
//!
//! Tests:
//! 1. Void bar handling: equity carry-forward, void bar rate tracking
//! 2. Warmup: indicator lookback respected, no activity before warmup
//! 3. Equity accounting: equity == cash + positions at every bar
//! 4. Precomputed-vs-naive: indicator values match when computed via engine

use chrono::NaiveDate;
use std::collections::HashMap;
use trendlab_core::components::execution::NextBarOpenModel;
use trendlab_core::components::filter::NoFilter;
use trendlab_core::components::indicator::Indicator;
use trendlab_core::components::pm::NoOpPm;
use trendlab_core::components::signal::NullSignal;
use trendlab_core::data::align::AlignedData;
use trendlab_core::data::provider::RawBar;
use trendlab_core::engine::{run_backtest, EngineConfig};
use trendlab_core::indicators::{Ema, Sma};

/// Helper: create aligned data for a single symbol.
fn make_aligned_single(symbol: &str, bars: Vec<RawBar>) -> AlignedData {
    let dates: Vec<NaiveDate> = bars.iter().map(|b| b.date).collect();
    let symbols = vec![symbol.to_string()];
    let mut bar_map = HashMap::new();
    bar_map.insert(symbol.to_string(), bars);
    AlignedData {
        dates,
        bars: bar_map,
        symbols,
    }
}

/// Helper: create N simple bars with linearly increasing prices.
fn simple_bars(n: usize) -> Vec<RawBar> {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    (0..n)
        .map(|i| {
            let close = 100.0 + i as f64;
            RawBar {
                date: base_date + chrono::Duration::days(i as i64),
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1000,
                adj_close: close,
            }
        })
        .collect()
}

/// Helper: create a void (NaN) bar at a specific date.
fn void_bar(date: NaiveDate) -> RawBar {
    RawBar {
        date,
        open: f64::NAN,
        high: f64::NAN,
        low: f64::NAN,
        close: f64::NAN,
        volume: 0,
        adj_close: f64::NAN,
    }
}

// ──────────────────────────────────────────────
// Void bar tests
// ──────────────────────────────────────────────

#[test]
fn void_bars_equity_carries_forward() {
    // SPY: 10 bars, indices 3-5 are void
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let mut bars = simple_bars(10);
    for i in 3..=5 {
        bars[i] = void_bar(base_date + chrono::Duration::days(i as i64));
    }

    let aligned = make_aligned_single("SPY", bars);
    let config = EngineConfig::new(100_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    // With no positions, equity should be constant throughout
    for &eq in &result.equity_curve {
        assert_eq!(eq, 100_000.0);
    }

    // Void bar rate = 3/10 = 30%
    let rate = result.void_bar_rates["SPY"];
    assert!(
        (rate - 0.3).abs() < 1e-10,
        "expected 30% void rate, got {rate}"
    );
}

#[test]
fn void_bars_trigger_data_quality_warning() {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let mut bars = simple_bars(10);
    // 3 void bars out of 10 = 30% > 10% threshold
    for i in 3..=5 {
        bars[i] = void_bar(base_date + chrono::Duration::days(i as i64));
    }

    let aligned = make_aligned_single("SPY", bars);
    let config = EngineConfig::new(100_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    assert!(
        !result.data_quality_warnings.is_empty(),
        "should have data quality warning for 30% void bars"
    );
    assert!(result.data_quality_warnings[0].contains("SPY"));
}

#[test]
fn void_bars_no_warning_under_threshold() {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let mut bars = simple_bars(100);
    // 5 void bars out of 100 = 5% < 10% threshold
    for i in 10..=14 {
        bars[i] = void_bar(base_date + chrono::Duration::days(i as i64));
    }

    let aligned = make_aligned_single("SPY", bars);
    let config = EngineConfig::new(100_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    // 5% void rate should not trigger a warning
    let rate = result.void_bar_rates["SPY"];
    assert!(
        (rate - 0.05).abs() < 1e-10,
        "expected 5% void rate, got {rate}"
    );
    assert!(
        result.data_quality_warnings.is_empty(),
        "5% void rate should not trigger warning"
    );
}

#[test]
fn void_bars_no_fills_generated() {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let mut bars = simple_bars(10);
    for i in 3..=5 {
        bars[i] = void_bar(base_date + chrono::Duration::days(i as i64));
    }

    let aligned = make_aligned_single("SPY", bars);
    let config = EngineConfig::new(100_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    // Phase 5b: no fills at all (order book not implemented yet)
    assert!(result.fills.is_empty());
    assert!(result.trades.is_empty());
}

// ──────────────────────────────────────────────
// Multi-symbol void bar tests
// ──────────────────────────────────────────────

#[test]
fn multi_symbol_independent_void_tracking() {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let dates: Vec<NaiveDate> = (0..10)
        .map(|i| base_date + chrono::Duration::days(i as i64))
        .collect();

    // SPY: no void bars
    let spy_bars: Vec<RawBar> = dates
        .iter()
        .enumerate()
        .map(|(i, &date)| {
            let close = 100.0 + i as f64;
            RawBar {
                date,
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1000,
                adj_close: close,
            }
        })
        .collect();

    // QQQ: 4 void bars at indices 2-5
    let mut qqq_bars: Vec<RawBar> = dates
        .iter()
        .enumerate()
        .map(|(i, &date)| {
            let close = 200.0 + i as f64;
            RawBar {
                date,
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 2000,
                adj_close: close,
            }
        })
        .collect();
    for i in 2..=5 {
        qqq_bars[i] = void_bar(dates[i]);
    }

    let mut bar_map = HashMap::new();
    bar_map.insert("SPY".to_string(), spy_bars);
    bar_map.insert("QQQ".to_string(), qqq_bars);

    let aligned = AlignedData {
        dates,
        bars: bar_map,
        symbols: vec!["SPY".to_string(), "QQQ".to_string()],
    };

    let config = EngineConfig::new(100_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    // SPY: 0% void
    assert!(
        result.void_bar_rates["SPY"].abs() < 1e-10,
        "SPY should have 0% void bars"
    );
    // QQQ: 40% void
    assert!(
        (result.void_bar_rates["QQQ"] - 0.4).abs() < 1e-10,
        "QQQ should have 40% void bars, got {}",
        result.void_bar_rates["QQQ"]
    );
}

// ──────────────────────────────────────────────
// Warmup tests
// ──────────────────────────────────────────────

#[test]
fn warmup_from_indicator_lookback() {
    let aligned = make_aligned_single("SPY", simple_bars(50));
    let indicators: Vec<Box<dyn Indicator>> = vec![
        Box::new(Sma::new(20)), // lookback = 19
        Box::new(Ema::new(10)), // lookback = 9
    ];
    let config = EngineConfig::new(100_000.0, 0);

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    // Warmup = max(19, 9) = 19
    assert_eq!(result.warmup_bars, 19);
}

#[test]
fn warmup_explicit_override_when_larger() {
    let aligned = make_aligned_single("SPY", simple_bars(50));
    let indicators: Vec<Box<dyn Indicator>> = vec![
        Box::new(Sma::new(5)), // lookback = 4
    ];
    let config = EngineConfig::new(100_000.0, 30); // explicit 30 > indicator's 4

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    assert_eq!(result.warmup_bars, 30);
}

#[test]
fn warmup_indicator_override_when_larger() {
    let aligned = make_aligned_single("SPY", simple_bars(50));
    let indicators: Vec<Box<dyn Indicator>> = vec![
        Box::new(Sma::new(20)), // lookback = 19
    ];
    let config = EngineConfig::new(100_000.0, 5); // explicit 5 < indicator's 19

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    assert_eq!(result.warmup_bars, 19);
}

#[test]
fn warmup_no_indicators() {
    let aligned = make_aligned_single("SPY", simple_bars(50));
    let indicators: Vec<Box<dyn Indicator>> = vec![];
    let config = EngineConfig::new(100_000.0, 0);

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    assert_eq!(result.warmup_bars, 0);
}

// ──────────────────────────────────────────────
// Equity accounting tests
// ──────────────────────────────────────────────

#[test]
fn equity_constant_with_no_positions() {
    let aligned = make_aligned_single("SPY", simple_bars(50));
    let config = EngineConfig::new(75_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    // With no positions, equity == initial capital at every bar
    for (i, &eq) in result.equity_curve.iter().enumerate() {
        assert_eq!(
            eq, 75_000.0,
            "equity should be constant at bar {i}, got {eq}"
        );
    }
    assert_eq!(result.final_equity, 75_000.0);
}

#[test]
fn equity_curve_length_matches_bar_count() {
    for n in [5, 25, 100] {
        let aligned = make_aligned_single("SPY", simple_bars(n));
        let config = EngineConfig::new(100_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![];

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        assert_eq!(result.equity_curve.len(), n);
        assert_eq!(result.bar_count, n);
    }
}

#[test]
fn equity_through_void_bars_with_no_positions() {
    // Even with void bars interspersed, flat portfolio equity stays constant
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let mut bars = simple_bars(20);
    for i in [3, 7, 8, 15] {
        bars[i] = void_bar(base_date + chrono::Duration::days(i as i64));
    }

    let aligned = make_aligned_single("SPY", bars);
    let config = EngineConfig::new(50_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![];

    let result = run_backtest(
        &aligned,
        &indicators,
        &config,
        &NullSignal,
        &NoFilter,
        &NextBarOpenModel::default(),
        &NoOpPm,
    );

    for (i, &eq) in result.equity_curve.iter().enumerate() {
        assert_eq!(
            eq, 50_000.0,
            "equity should be constant at bar {i}, got {eq}"
        );
    }
}

// ──────────────────────────────────────────────
// Precomputed indicator consistency
// ──────────────────────────────────────────────

#[test]
fn precomputed_indicators_match_direct_computation() {
    // Verify that the engine's precompute produces the same values as
    // direct Indicator::compute() on the same bars.
    use trendlab_core::domain::Bar;
    use trendlab_core::engine::precompute_indicators;

    let raw_bars = simple_bars(50);

    // Convert RawBar to Bar manually for direct comparison
    let domain_bars: Vec<Bar> = raw_bars
        .iter()
        .map(|r| Bar {
            symbol: "SPY".to_string(),
            date: r.date,
            open: r.open,
            high: r.high,
            low: r.low,
            close: r.close,
            volume: r.volume,
            adj_close: r.adj_close,
        })
        .collect();

    let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(Sma::new(10)), Box::new(Ema::new(5))];

    // Direct computation
    let sma_direct = Sma::new(10).compute(&domain_bars);
    let ema_direct = Ema::new(5).compute(&domain_bars);

    // Engine precomputation
    let mut bars_by_symbol = HashMap::new();
    bars_by_symbol.insert("SPY".to_string(), domain_bars);
    let precomputed = precompute_indicators(&bars_by_symbol, &indicators);

    let iv = &precomputed["SPY"];
    for i in 0..50 {
        let sma_pre = iv.get("sma_10", i);
        let ema_pre = iv.get("ema_5", i);

        // Compare SMA
        if sma_direct[i].is_nan() {
            assert!(
                sma_pre.is_none() || sma_pre.unwrap().is_nan(),
                "SMA should be NaN at bar {i}"
            );
        } else {
            let pre = sma_pre.expect("SMA should exist in precomputed");
            assert!(
                (pre - sma_direct[i]).abs() < 1e-10,
                "SMA mismatch at bar {i}: precomputed={pre}, direct={}",
                sma_direct[i]
            );
        }

        // Compare EMA
        if ema_direct[i].is_nan() {
            assert!(
                ema_pre.is_none() || ema_pre.unwrap().is_nan(),
                "EMA should be NaN at bar {i}"
            );
        } else {
            let pre = ema_pre.expect("EMA should exist in precomputed");
            assert!(
                (pre - ema_direct[i]).abs() < 1e-10,
                "EMA mismatch at bar {i}: precomputed={pre}, direct={}",
                ema_direct[i]
            );
        }
    }
}
