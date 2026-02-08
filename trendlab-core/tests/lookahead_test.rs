//! Look-ahead contamination tests for all 13 indicators.
//!
//! Invariant (from CLAUDE.md):
//! No indicator value at bar t may depend on price data from bar t+1 or later.
//!
//! Method: compute on truncated series (bars 0..100) and full series (bars 0..200).
//! Assert bars 0..100 are identical between both runs. Any difference means the
//! indicator is leaking future data into past values.

use chrono::NaiveDate;
use trendlab_core::components::indicator::Indicator;
use trendlab_core::domain::Bar;
use trendlab_core::indicators::*;

/// Generate N bars of synthetic OHLCV data with realistic variation.
fn make_test_bars(n: usize) -> Vec<Bar> {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let mut bars = Vec::with_capacity(n);
    let mut price = 100.0;

    for i in 0..n {
        // Deterministic pseudo-random walk using a simple LCG
        let seed = (i as u64).wrapping_mul(6364136223846793005).wrapping_add(1);
        let change = ((seed % 200) as f64 - 100.0) * 0.05; // -5.0 to +5.0
        price += change;
        price = price.max(10.0); // floor at 10

        let open = price - 0.5;
        let close = price + 0.3;
        let high = open.max(close) + 2.0;
        let low = open.min(close) - 2.0;

        bars.push(Bar {
            symbol: "TEST".to_string(),
            date: base_date + chrono::Duration::days(i as i64),
            open,
            high,
            low,
            close,
            volume: 1000 + (i as u64 * 100),
            adj_close: close,
        });
    }

    bars
}

/// Assert that the indicator produces identical values for bars 0..truncated_len
/// whether computed on a truncated or full series.
fn assert_no_lookahead(indicator: &dyn Indicator, full_bars: &[Bar], truncated_len: usize) {
    let truncated = &full_bars[..truncated_len];
    let full_result = indicator.compute(full_bars);
    let truncated_result = indicator.compute(truncated);

    assert_eq!(
        truncated_result.len(),
        truncated_len,
        "{}: truncated result length mismatch",
        indicator.name()
    );
    assert_eq!(
        full_result.len(),
        full_bars.len(),
        "{}: full result length mismatch",
        indicator.name()
    );

    for i in 0..truncated_len {
        let t = truncated_result[i];
        let f = full_result[i];

        if t.is_nan() && f.is_nan() {
            continue;
        }

        assert!(
            !t.is_nan() && !f.is_nan(),
            "{}: NaN mismatch at bar {i} (truncated={t}, full={f})",
            indicator.name()
        );

        assert!(
            (t - f).abs() < 1e-10,
            "{}: look-ahead contamination at bar {i}: truncated={t}, full={f}, diff={}",
            indicator.name(),
            (t - f).abs()
        );
    }
}

#[test]
fn lookahead_sma() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Sma::new(10), &bars, 100);
    assert_no_lookahead(&Sma::new(20), &bars, 100);
}

#[test]
fn lookahead_ema() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Ema::new(10), &bars, 100);
    assert_no_lookahead(&Ema::new(20), &bars, 100);
}

#[test]
fn lookahead_roc() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Roc::new(5), &bars, 100);
    assert_no_lookahead(&Roc::new(10), &bars, 100);
}

#[test]
fn lookahead_momentum() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Momentum::new(5), &bars, 100);
    assert_no_lookahead(&Momentum::new(10), &bars, 100);
}

#[test]
fn lookahead_donchian() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Donchian::upper(10), &bars, 100);
    assert_no_lookahead(&Donchian::lower(10), &bars, 100);
    assert_no_lookahead(&Donchian::upper(20), &bars, 100);
    assert_no_lookahead(&Donchian::lower(20), &bars, 100);
}

#[test]
fn lookahead_atr() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Atr::new(14), &bars, 100);
    assert_no_lookahead(&Atr::new(5), &bars, 100);
}

#[test]
fn lookahead_rsi() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Rsi::new(14), &bars, 100);
    assert_no_lookahead(&Rsi::new(7), &bars, 100);
}

#[test]
fn lookahead_bollinger() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Bollinger::upper(20, 2.0), &bars, 100);
    assert_no_lookahead(&Bollinger::middle(20, 2.0), &bars, 100);
    assert_no_lookahead(&Bollinger::lower(20, 2.0), &bars, 100);
}

#[test]
fn lookahead_aroon() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Aroon::up(10), &bars, 100);
    assert_no_lookahead(&Aroon::down(10), &bars, 100);
    assert_no_lookahead(&Aroon::up(25), &bars, 100);
    assert_no_lookahead(&Aroon::down(25), &bars, 100);
}

#[test]
fn lookahead_keltner() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Keltner::upper(20, 10, 1.5), &bars, 100);
    assert_no_lookahead(&Keltner::middle(20, 10, 1.5), &bars, 100);
    assert_no_lookahead(&Keltner::lower(20, 10, 1.5), &bars, 100);
}

#[test]
fn lookahead_adx() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Adx::new(14), &bars, 100);
    assert_no_lookahead(&Adx::new(7), &bars, 100);
}

#[test]
fn lookahead_supertrend() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&Supertrend::new(10, 3.0), &bars, 100);
    assert_no_lookahead(&Supertrend::new(7, 2.0), &bars, 100);
}

#[test]
fn lookahead_parabolic_sar() {
    let bars = make_test_bars(200);
    assert_no_lookahead(&ParabolicSar::default_params(), &bars, 100);
}
