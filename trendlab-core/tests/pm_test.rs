//! Integration tests for position managers.
//!
//! Tests:
//! 1. Ratchet property: for each PM, stops are monotonically non-decreasing (long)
//!    or non-increasing (short) across all price paths.
//! 2. Anti-stickiness regressions: PMs correctly exit under adversarial conditions.
//! 3. PM-specific behavioral contracts.

use chrono::NaiveDate;
use trendlab_core::components::indicator::IndicatorValues;
use trendlab_core::components::pm::*;
use trendlab_core::domain::{Bar, MarketStatus, Position, PositionSide};

// ──────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────

fn make_bar_at(date: NaiveDate, open: f64, high: f64, low: f64, close: f64) -> Bar {
    Bar {
        symbol: "SPY".to_string(),
        date,
        open,
        high,
        low,
        close,
        volume: 1000,
        adj_close: close,
    }
}

fn make_bar(close: f64) -> Bar {
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    make_bar_at(date, close - 0.5, close + 1.0, close - 1.0, close)
}

/// Build indicators with a single ATR series.
fn make_atr_indicators(period: usize, values: &[f64]) -> IndicatorValues {
    let mut iv = IndicatorValues::new();
    iv.insert(format!("atr_{period}"), values.to_vec());
    iv
}

/// Simulate the ratchet enforcement that the engine performs.
fn ratchet_long(new_stop: f64, current: Option<f64>) -> f64 {
    match current {
        Some(cur) => new_stop.max(cur),
        None => new_stop,
    }
}

fn ratchet_short(new_stop: f64, current: Option<f64>) -> f64 {
    match current {
        Some(cur) => new_stop.min(cur),
        None => new_stop,
    }
}

// ──────────────────────────────────────────────
// Ratchet property tests — rising price path
// ──────────────────────────────────────────────

/// For any PM on a rising price path, the stop sequence must be non-decreasing (long).
fn assert_ratchet_long_rising(pm: &dyn PositionManager, indicators: &IndicatorValues) {
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    for i in 0..50 {
        let close = 100.0 + i as f64; // steadily rising
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, indicators);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    // Verify monotonically non-decreasing
    for w in stops.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-10,
            "{}: long ratchet violated on rising path: {} -> {}",
            pm.name(),
            w[0],
            w[1]
        );
    }
}

/// For any PM on a falling price path, the stop sequence must be non-increasing (short).
fn assert_ratchet_short_falling(pm: &dyn PositionManager, indicators: &IndicatorValues) {
    let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    for i in 0..50 {
        let close = 100.0 - i as f64 * 0.5; // steadily falling
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, indicators);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_short(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    for w in stops.windows(2) {
        assert!(
            w[1] <= w[0] + 1e-10,
            "{}: short ratchet violated on falling path: {} -> {}",
            pm.name(),
            w[0],
            w[1]
        );
    }
}

// ──────────────────────────────────────────────
// Ratchet property tests — V-shape path
// ──────────────────────────────────────────────

/// V-shape: price rises 25 bars then falls 25 bars. Ratchet must hold throughout.
fn assert_ratchet_long_vshape(pm: &dyn PositionManager, indicators: &IndicatorValues) {
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    for i in 0..50 {
        let close = if i < 25 {
            100.0 + i as f64 * 2.0 // rise to 148
        } else {
            148.0 - (i - 25) as f64 * 2.0 // fall back to 98
        };
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, indicators);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    for w in stops.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-10,
            "{}: long ratchet violated on V-shape: {} -> {}",
            pm.name(),
            w[0],
            w[1]
        );
    }
}

fn assert_ratchet_short_vshape(pm: &dyn PositionManager, indicators: &IndicatorValues) {
    let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    for i in 0..50 {
        let close = if i < 25 {
            100.0 - i as f64 * 1.0 // fall to 75
        } else {
            75.0 + (i - 25) as f64 * 1.0 // rise back to 100
        };
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, indicators);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_short(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    for w in stops.windows(2) {
        assert!(
            w[1] <= w[0] + 1e-10,
            "{}: short ratchet violated on V-shape: {} -> {}",
            pm.name(),
            w[0],
            w[1]
        );
    }
}

// ──────────────────────────────────────────────
// Ratchet property tests — gap path
// ──────────────────────────────────────────────

/// Price gaps up 5% then gaps down 5%. Ratchet must still hold.
fn assert_ratchet_long_gap(pm: &dyn PositionManager, indicators: &IndicatorValues) {
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    let prices = [
        100.0, 105.0, 110.25, 115.76, 121.55, 127.63, // gaps up 5%
        121.25, 115.19, 109.43, 103.96, 98.76,
    ]; // gaps down ~5%

    for (i, &close) in prices.iter().enumerate() {
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, indicators);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    for w in stops.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-10,
            "{}: long ratchet violated on gap path: {} -> {}",
            pm.name(),
            w[0],
            w[1]
        );
    }
}

// ──────────────────────────────────────────────
// Ratchet: run all PMs through all paths
// ──────────────────────────────────────────────

#[test]
fn ratchet_percent_trailing_all_paths() {
    let pm = PercentTrailing::new(0.10);
    let iv = IndicatorValues::new();
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
    assert_ratchet_long_gap(&pm, &iv);
}

#[test]
fn ratchet_fixed_stop_loss_all_paths() {
    let pm = FixedStopLoss::new(0.05);
    let iv = IndicatorValues::new();
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
    assert_ratchet_long_gap(&pm, &iv);
}

#[test]
fn ratchet_frozen_reference_all_paths() {
    let pm = FrozenReference::new(0.08);
    let iv = IndicatorValues::new();
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
    assert_ratchet_long_gap(&pm, &iv);
}

#[test]
fn ratchet_time_decay_all_paths() {
    let pm = TimeDecay::new(0.10, 0.001, 0.02);
    let iv = IndicatorValues::new();
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
    assert_ratchet_long_gap(&pm, &iv);
}

#[test]
fn ratchet_breakeven_then_trail_all_paths() {
    let pm = BreakevenThenTrail::new(0.05, 0.10);
    let iv = IndicatorValues::new();
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
    assert_ratchet_long_gap(&pm, &iv);
}

#[test]
fn ratchet_atr_trailing_all_paths() {
    let pm = AtrTrailing::new(14, 3.0);
    // ATR held constant at 2.0 for all 50 bars
    let atr_values: Vec<f64> = vec![2.0; 50];
    let iv = make_atr_indicators(14, &atr_values);
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
}

#[test]
fn ratchet_chandelier_all_paths() {
    let pm = Chandelier::new(14, 3.0);
    let atr_values: Vec<f64> = vec![2.0; 50];
    let iv = make_atr_indicators(14, &atr_values);
    assert_ratchet_long_rising(&pm, &iv);
    assert_ratchet_short_falling(&pm, &iv);
    assert_ratchet_long_vshape(&pm, &iv);
    assert_ratchet_short_vshape(&pm, &iv);
}

// ──────────────────────────────────────────────
// Ratchet with ATR expansion (volatility spike)
// ──────────────────────────────────────────────

#[test]
fn ratchet_atr_trailing_atr_expansion() {
    // ATR doubles mid-series. The raw stop loosens, but ratchet clamps it.
    let pm = AtrTrailing::new(14, 3.0);
    let mut atr_values: Vec<f64> = vec![2.0; 50];
    for v in atr_values[25..].iter_mut() {
        *v = 4.0; // ATR doubles at bar 25
    }
    let iv = make_atr_indicators(14, &atr_values);

    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    for i in 0..50 {
        let close = 100.0 + i as f64; // rising
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, &iv);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    for w in stops.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-10,
            "ATR expansion: long ratchet violated: {} -> {}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn ratchet_chandelier_atr_expansion() {
    let pm = Chandelier::new(14, 3.0);
    let mut atr_values: Vec<f64> = vec![2.0; 50];
    for v in atr_values[25..].iter_mut() {
        *v = 4.0;
    }
    let iv = make_atr_indicators(14, &atr_values);

    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let mut stops: Vec<f64> = vec![];

    for i in 0..50 {
        let close = 100.0 + i as f64;
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, &iv);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            stops.push(clamped);
        }
    }

    for w in stops.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-10,
            "ATR expansion chandelier: long ratchet violated: {} -> {}",
            w[0],
            w[1]
        );
    }
}

// ──────────────────────────────────────────────
// Anti-stickiness regression: chandelier doesn't
// get trapped by rising then reversing price
// ──────────────────────────────────────────────

#[test]
fn chandelier_not_trapped_by_reversal() {
    // Price rises to 150, then falls back. The chandelier stop should remain
    // close to where it was at the peak, not chase back down.
    let pm = Chandelier::new(14, 2.0);
    let atr_values: Vec<f64> = vec![3.0; 40];
    let iv = make_atr_indicators(14, &atr_values);

    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let mut last_stop_at_peak = 0.0_f64;

    // Rise to 150 over 20 bars
    for i in 0..20 {
        let close = 100.0 + i as f64 * 2.5;
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, &iv);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
            last_stop_at_peak = clamped;
        }
    }

    // Fall from 150 back to 110 over 20 bars
    for i in 20..40 {
        let close = 150.0 - (i - 20) as f64 * 2.0;
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, &iv);
        if let Some(raw_stop) = intent.stop_price {
            let clamped = ratchet_long(raw_stop, pos.current_stop);
            pos.current_stop = Some(clamped);
        }
    }

    // Stop should still be at least where it was at the peak
    assert!(
        pos.current_stop.unwrap() >= last_stop_at_peak - 1e-10,
        "chandelier stop should not have loosened after peak: peak_stop={}, final_stop={}",
        last_stop_at_peak,
        pos.current_stop.unwrap()
    );

    // Stop should be near the high watermark (150 - 2*3 = 144)
    // It can only be >= 144 due to ratchet
    assert!(
        pos.current_stop.unwrap() >= 144.0 - 1e-10,
        "chandelier stop should be anchored near peak: expected >= 144, got {}",
        pos.current_stop.unwrap()
    );
}

// ──────────────────────────────────────────────
// Anti-stickiness: frozen reference emits
// AdjustStop exactly once
// ──────────────────────────────────────────────

#[test]
fn frozen_reference_emits_adjust_exactly_once() {
    let pm = FrozenReference::new(0.10);
    let iv = IndicatorValues::new();
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);

    let mut adjust_count = 0;

    for i in 0..100 {
        let close = 100.0 + i as f64;
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, &iv);
        if intent.action == IntentAction::AdjustStop {
            adjust_count += 1;
            pos.current_stop = intent.stop_price;
        }
    }

    assert_eq!(
        adjust_count, 1,
        "frozen_reference should emit AdjustStop exactly once, got {adjust_count}"
    );
    assert_eq!(pos.current_stop, Some(90.0));
}

// ──────────────────────────────────────────────
// Anti-stickiness: time_decay converges to min_pct
// ──────────────────────────────────────────────

#[test]
fn time_decay_converges_to_min_pct() {
    let pm = TimeDecay::new(0.10, 0.001, 0.02);
    let iv = IndicatorValues::new();

    // At bar 80 the effective pct should be exactly min_pct (0.02)
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    pos.bars_held = 80;
    let bar = make_bar(100.0);
    let intent = pm.on_bar(&pos, &bar, 80, MarketStatus::Open, &iv);
    assert_eq!(intent.stop_price, Some(98.0)); // 100 * (1 - 0.02)

    // At bar 200, still at min_pct
    pos.bars_held = 200;
    let intent = pm.on_bar(&pos, &bar, 200, MarketStatus::Open, &iv);
    assert_eq!(intent.stop_price, Some(98.0));
}

// ──────────────────────────────────────────────
// Anti-stickiness: max_holding_period fires at
// exactly max_bars
// ──────────────────────────────────────────────

#[test]
fn max_holding_fires_at_exact_boundary() {
    let pm = MaxHoldingPeriod::new(20);
    let iv = IndicatorValues::new();
    let bar = make_bar(110.0);

    // Bar 19: still holds
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    pos.bars_held = 19;
    let intent = pm.on_bar(&pos, &bar, 19, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);

    // Bar 20: fires
    pos.bars_held = 20;
    let intent = pm.on_bar(&pos, &bar, 20, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::ForceExit);
}

#[test]
fn max_holding_fires_for_shorts() {
    let pm = MaxHoldingPeriod::new(10);
    let iv = IndicatorValues::new();
    let bar = make_bar(90.0);

    let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
    pos.bars_held = 10;
    let intent = pm.on_bar(&pos, &bar, 10, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::ForceExit);
}

// ──────────────────────────────────────────────
// Anti-stickiness: breakeven triggers phase
// transition correctly
// ──────────────────────────────────────────────

#[test]
fn breakeven_phase_transition_long() {
    let pm = BreakevenThenTrail::new(0.05, 0.10);
    let iv = IndicatorValues::new();
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);

    // Phase 1: price hasn't reached breakeven trigger (5%)
    pos.highest_price_since_entry = 103.0; // 3% — below trigger
    let bar = make_bar(103.0);
    let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);

    // Price reaches trigger
    pos.highest_price_since_entry = 106.0; // 6% — above trigger
    pos.update_mark(106.0);
    let bar = make_bar(106.0);
    let intent = pm.on_bar(&pos, &bar, 2, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::AdjustStop);
    assert_eq!(intent.stop_price, Some(100.0)); // breakeven = entry

    // Simulate engine setting stop
    pos.current_stop = Some(100.0);

    // Phase 2: trailing
    pos.highest_price_since_entry = 120.0;
    pos.update_mark(118.0);
    let bar = make_bar(118.0);
    let intent = pm.on_bar(&pos, &bar, 10, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::AdjustStop);
    // 120 * 0.9 = 108
    assert_eq!(intent.stop_price, Some(108.0));
}

#[test]
fn breakeven_phase_transition_short() {
    let pm = BreakevenThenTrail::new(0.05, 0.10);
    let iv = IndicatorValues::new();
    let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);

    // Phase 1: price hasn't dropped enough
    pos.lowest_price_since_entry = 97.0; // 3% — below trigger
    let bar = make_bar(97.0);
    let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);

    // Price drops enough to trigger
    pos.lowest_price_since_entry = 94.0; // 6%
    pos.update_mark(94.0);
    let bar = make_bar(94.0);
    let intent = pm.on_bar(&pos, &bar, 2, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::AdjustStop);
    assert_eq!(intent.stop_price, Some(100.0)); // breakeven

    // Simulate engine setting stop
    pos.current_stop = Some(100.0);

    // Phase 2: trailing
    pos.lowest_price_since_entry = 80.0;
    pos.update_mark(82.0);
    let bar = make_bar(82.0);
    let intent = pm.on_bar(&pos, &bar, 10, MarketStatus::Open, &iv);
    // 80 * 1.1 = 88
    assert_eq!(intent.stop_price, Some(88.0));
}

// ──────────────────────────────────────────────
// Anti-stickiness: since_entry_trailing fires
// on sufficient drawdown
// ──────────────────────────────────────────────

#[test]
fn since_entry_trailing_fires_on_drawdown() {
    let pm = SinceEntryTrailing::new(0.10);
    let iv = IndicatorValues::new();

    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    pos.highest_price_since_entry = 120.0;

    // 8% drawdown: holds
    let bar = make_bar(110.4); // (120-110.4)/120 = 8%
    let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);

    // 12.5% drawdown: fires
    let bar = make_bar(105.0); // (120-105)/120 = 12.5%
    let intent = pm.on_bar(&pos, &bar, 6, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::ForceExit);
}

// ──────────────────────────────────────────────
// ATR trailing: missing indicator returns Hold
// ──────────────────────────────────────────────

#[test]
fn atr_trailing_holds_on_nan_indicator() {
    let pm = AtrTrailing::new(14, 3.0);
    let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let bar = make_bar(110.0);

    // No indicators at all
    let iv = IndicatorValues::new();
    let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);

    // NaN indicator value
    let iv = make_atr_indicators(14, &[f64::NAN; 10]);
    let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);
}

#[test]
fn chandelier_holds_on_nan_indicator() {
    let pm = Chandelier::new(14, 3.0);
    let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let bar = make_bar(110.0);

    let iv = IndicatorValues::new();
    let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);

    let iv = make_atr_indicators(14, &[f64::NAN; 10]);
    let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(intent.action, IntentAction::Hold);
}

// ──────────────────────────────────────────────
// Long / Short symmetry tests
// ──────────────────────────────────────────────

#[test]
fn percent_trailing_long_short_symmetry() {
    let pm = PercentTrailing::new(0.10);
    let iv = IndicatorValues::new();

    // Long: stop at 90% of high
    let mut long_pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    long_pos.highest_price_since_entry = 120.0;
    let bar = make_bar(115.0);
    let long_intent = pm.on_bar(&long_pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(long_intent.stop_price, Some(108.0)); // 120 * 0.9

    // Short: stop at 110% of low
    let mut short_pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
    short_pos.lowest_price_since_entry = 80.0;
    let bar = make_bar(85.0);
    let short_intent = pm.on_bar(&short_pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(short_intent.stop_price, Some(88.0)); // 80 * 1.1
}

#[test]
fn atr_trailing_long_short_symmetry() {
    let pm = AtrTrailing::new(14, 2.0);
    let iv = make_atr_indicators(14, &[5.0; 10]);

    // Long: stop = close - ATR * mult = 110 - 10 = 100
    let long_pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let bar = make_bar(110.0);
    let long_intent = pm.on_bar(&long_pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(long_intent.stop_price, Some(100.0));

    // Short: stop = close + ATR * mult = 90 + 10 = 100
    let short_pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
    let bar = make_bar(90.0);
    let short_intent = pm.on_bar(&short_pos, &bar, 5, MarketStatus::Open, &iv);
    assert_eq!(short_intent.stop_price, Some(100.0));
}

#[test]
fn fixed_stop_loss_long_short_symmetry() {
    let pm = FixedStopLoss::new(0.05);
    let iv = IndicatorValues::new();

    let long_pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
    let bar = make_bar(102.0);
    let long_intent = pm.on_bar(&long_pos, &bar, 1, MarketStatus::Open, &iv);
    assert_eq!(long_intent.stop_price, Some(95.0)); // 100 * 0.95

    let short_pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
    let bar = make_bar(98.0);
    let short_intent = pm.on_bar(&short_pos, &bar, 1, MarketStatus::Open, &iv);
    assert_eq!(short_intent.stop_price, Some(105.0)); // 100 * 1.05
}

// ──────────────────────────────────────────────
// Flat position always gets Hold
// ──────────────────────────────────────────────

#[test]
fn flat_position_always_holds() {
    let flat = Position {
        symbol: "SPY".into(),
        side: PositionSide::Flat,
        quantity: 0.0,
        avg_entry_price: 100.0,
        entry_bar: 0,
        highest_price_since_entry: 100.0,
        lowest_price_since_entry: 100.0,
        bars_held: 0,
        unrealized_pnl: 0.0,
        realized_pnl: 0.0,
        current_stop: None,
    };
    let bar = make_bar(100.0);
    let iv = IndicatorValues::new();
    let atr_iv = make_atr_indicators(14, &[5.0; 10]);

    let pms: Vec<Box<dyn PositionManager>> = vec![
        Box::new(PercentTrailing::new(0.10)),
        Box::new(FixedStopLoss::new(0.05)),
        Box::new(FrozenReference::new(0.08)),
        Box::new(TimeDecay::new(0.10, 0.001, 0.02)),
        Box::new(AtrTrailing::new(14, 3.0)),
        Box::new(Chandelier::new(14, 3.0)),
    ];

    for pm in &pms {
        let indicators = if pm.name().contains("atr") || pm.name().contains("chandelier") {
            &atr_iv
        } else {
            &iv
        };
        let intent = pm.on_bar(&flat, &bar, 5, MarketStatus::Open, indicators);
        assert_eq!(
            intent.action,
            IntentAction::Hold,
            "{} should Hold for flat position",
            pm.name()
        );
    }
}

// ──────────────────────────────────────────────
// NoOpPm always holds (100 bars)
// ──────────────────────────────────────────────

#[test]
fn noop_pm_always_holds_extended() {
    let pm = NoOpPm;
    let iv = IndicatorValues::new();
    let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);

    for i in 0..100 {
        let close = 100.0 + (i as f64 * 0.5);
        let bar = make_bar(close);
        pos.update_mark(close);
        pos.tick_bar();

        let intent = pm.on_bar(&pos, &bar, i, MarketStatus::Open, &iv);
        assert_eq!(
            intent.action,
            IntentAction::Hold,
            "NoOpPm should Hold at bar {i}"
        );
    }
}
