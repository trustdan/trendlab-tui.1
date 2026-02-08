//! Moving average crossover signal — golden cross and death cross detection.
//!
//! Fires Long when the fast MA crosses above the slow MA (golden cross).
//! Fires Short when the fast MA crosses below the slow MA (death cross).

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Moving average type selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaType {
    Sma,
    Ema,
}

impl MaType {
    fn prefix(&self) -> &'static str {
        match self {
            MaType::Sma => "sma",
            MaType::Ema => "ema",
        }
    }
}

/// Moving average crossover signal generator.
///
/// Monitors two moving averages (fast and slow) and detects crossover events.
/// A golden cross (fast crosses above slow) emits a Long signal.
/// A death cross (fast crosses below slow) emits a Short signal.
///
/// # Indicator dependencies
/// Requires two precomputed MA series:
/// - Fast: `{ma_type}_{fast_period}` (e.g., `sma_10`)
/// - Slow: `{ma_type}_{slow_period}` (e.g., `sma_50`)
#[derive(Debug, Clone)]
pub struct MaCrossover {
    pub fast_period: usize,
    pub slow_period: usize,
    pub ma_type: MaType,
    fast_key: String,
    slow_key: String,
}

impl MaCrossover {
    pub fn new(fast_period: usize, slow_period: usize, ma_type: MaType) -> Self {
        assert!(fast_period >= 1, "fast_period must be >= 1");
        assert!(
            slow_period > fast_period,
            "slow_period must be > fast_period"
        );

        let prefix = ma_type.prefix();
        let fast_key = format!("{prefix}_{fast_period}");
        let slow_key = format!("{prefix}_{slow_period}");

        Self {
            fast_period,
            slow_period,
            ma_type,
            fast_key,
            slow_key,
        }
    }

    pub fn default_params() -> Self {
        Self::new(10, 50, MaType::Sma)
    }
}

impl SignalGenerator for MaCrossover {
    fn name(&self) -> &str {
        "ma_crossover"
    }

    fn warmup_bars(&self) -> usize {
        self.slow_period
    }

    fn evaluate(
        &self,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> Option<SignalEvent> {
        // Guard: need at least 2 bars for crossover detection, and must meet warmup.
        if bar_index == 0 || bar_index < self.warmup_bars() {
            return None;
        }

        let bar = &bars[bar_index];

        // NaN guard: current bar close.
        if bar.close.is_nan() {
            return None;
        }

        // Fetch all four indicator values.
        let fast_cur = indicators.get(&self.fast_key, bar_index)?;
        let slow_cur = indicators.get(&self.slow_key, bar_index)?;
        let fast_prev = indicators.get(&self.fast_key, bar_index - 1)?;
        let slow_prev = indicators.get(&self.slow_key, bar_index - 1)?;

        // NaN guard: all four indicator values must be valid.
        if fast_cur.is_nan() || slow_cur.is_nan() || fast_prev.is_nan() || slow_prev.is_nan() {
            return None;
        }

        let mut metadata = HashMap::new();
        metadata.insert("reference_price".into(), bar.close);
        metadata.insert("signal_bar_low".into(), bar.low);

        // Long signal (golden cross): fast crosses above slow.
        // Current bar: fast > slow. Previous bar: fast <= slow.
        if fast_cur > slow_cur && fast_prev <= slow_prev {
            return Some(SignalEvent {
                id: SignalEventId(0),
                bar_index,
                date: bar.date,
                symbol: bar.symbol.clone(),
                direction: SignalDirection::Long,
                strength: 1.0,
                metadata,
            });
        }

        // Short signal (death cross): fast crosses below slow.
        // Current bar: fast < slow. Previous bar: fast >= slow.
        if fast_cur < slow_cur && fast_prev >= slow_prev {
            return Some(SignalEvent {
                id: SignalEventId(0),
                bar_index,
                date: bar.date,
                symbol: bar.symbol.clone(),
                direction: SignalDirection::Short,
                strength: 1.0,
                metadata,
            });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn base_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
    }

    fn make_bar(index: usize, close: f64, low: f64) -> Bar {
        Bar {
            symbol: "AAPL".to_string(),
            date: base_date() + chrono::Duration::days(index as i64),
            open: close - 0.5,
            high: close + 1.0,
            low,
            close,
            volume: 2000,
            adj_close: close,
        }
    }

    fn make_indicators_two(
        fast_key: &str,
        fast_vals: Vec<f64>,
        slow_key: &str,
        slow_vals: Vec<f64>,
    ) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(fast_key.to_string(), fast_vals);
        iv.insert(slow_key.to_string(), slow_vals);
        iv
    }

    /// Helper: build a set of bars and indicators where a golden cross occurs at `cross_bar`.
    ///
    /// Before `cross_bar`: fast <= slow. At `cross_bar`: fast > slow.
    fn setup_golden_cross(n: usize, cross_bar: usize) -> (Vec<Bar>, IndicatorValues) {
        let bars: Vec<Bar> = (0..n)
            .map(|i| make_bar(i, 100.0 + i as f64, 98.0))
            .collect();

        let mut fast = vec![f64::NAN; n];
        let mut slow = vec![f64::NAN; n];
        // Before cross: fast <= slow.
        for i in 0..cross_bar {
            fast[i] = 95.0;
            slow[i] = 100.0;
        }
        // At cross: fast > slow.
        fast[cross_bar] = 105.0;
        slow[cross_bar] = 100.0;
        // After cross (if any).
        for i in (cross_bar + 1)..n {
            fast[i] = 106.0;
            slow[i] = 100.0;
        }

        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);
        (bars, iv)
    }

    /// Helper: build a set of bars and indicators where a death cross occurs at `cross_bar`.
    fn setup_death_cross(n: usize, cross_bar: usize) -> (Vec<Bar>, IndicatorValues) {
        let bars: Vec<Bar> = (0..n)
            .map(|i| make_bar(i, 100.0 + i as f64, 98.0))
            .collect();

        let mut fast = vec![f64::NAN; n];
        let mut slow = vec![f64::NAN; n];
        // Before cross: fast >= slow.
        for i in 0..cross_bar {
            fast[i] = 105.0;
            slow[i] = 100.0;
        }
        // At cross: fast < slow.
        fast[cross_bar] = 95.0;
        slow[cross_bar] = 100.0;
        // After cross (if any).
        for i in (cross_bar + 1)..n {
            fast[i] = 94.0;
            slow[i] = 100.0;
        }

        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);
        (bars, iv)
    }

    #[test]
    fn fires_long_on_golden_cross() {
        let (bars, iv) = setup_golden_cross(60, 52);
        let sig = MaCrossover::default_params(); // warmup = 50

        let result = sig.evaluate(&bars, 52, &iv);
        assert!(result.is_some(), "expected Long signal on golden cross");
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.bar_index, 52);
        assert_eq!(event.symbol, "AAPL");
        assert_eq!(event.metadata["reference_price"], 152.0); // 100.0 + 52
        assert_eq!(event.metadata["signal_bar_low"], 98.0);
        // No breakout_level for crossover signals.
        assert!(!event.metadata.contains_key("breakout_level"));
    }

    #[test]
    fn fires_short_on_death_cross() {
        let (bars, iv) = setup_death_cross(60, 52);
        let sig = MaCrossover::default_params();

        let result = sig.evaluate(&bars, 52, &iv);
        assert!(result.is_some(), "expected Short signal on death cross");
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Short);
        assert_eq!(event.bar_index, 52);
    }

    #[test]
    fn no_fire_when_trend_continues() {
        // Fast stays above slow across bar 51 and 52 (no crossover).
        let bars: Vec<Bar> = (0..60).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let fast = vec![105.0; 60];
        let slow = vec![100.0; 60];
        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn warmup_guard_bar_zero() {
        let bars = vec![make_bar(0, 100.0, 98.0)];
        let iv = make_indicators_two("sma_10", vec![100.0], "sma_50", vec![95.0]);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 0, &iv).is_none());
    }

    #[test]
    fn warmup_guard_below_slow_period() {
        // warmup = 50, so bar_index 49 should not fire.
        let bars: Vec<Bar> = (0..55).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let fast = vec![105.0; 55];
        let slow = vec![100.0; 55];
        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 49, &iv).is_none());
    }

    #[test]
    fn nan_guard_fast_current() {
        let (bars, _) = setup_golden_cross(60, 52);
        // Rebuild with NaN in fast at bar 52.
        let mut fast = vec![95.0; 60];
        fast[52] = f64::NAN;
        let slow = vec![100.0; 60];
        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn nan_guard_slow_current() {
        let bars: Vec<Bar> = (0..60).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let fast = vec![105.0; 60];
        let mut slow = vec![100.0; 60];
        slow[52] = f64::NAN;
        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn nan_guard_fast_previous() {
        let bars: Vec<Bar> = (0..60).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let mut fast = vec![95.0; 60];
        fast[51] = f64::NAN;
        fast[52] = 105.0;
        let slow = vec![100.0; 60];
        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn nan_guard_slow_previous() {
        let bars: Vec<Bar> = (0..60).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let fast = vec![105.0; 60];
        let mut slow = vec![100.0; 60];
        slow[51] = f64::NAN;
        let iv = make_indicators_two("sma_10", fast, "sma_50", slow);

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn nan_guard_bar_close() {
        let (mut bars, iv) = setup_golden_cross(60, 52);
        bars[52].close = f64::NAN;

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn missing_indicator_returns_none() {
        let bars: Vec<Bar> = (0..60).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let iv = IndicatorValues::new();

        let sig = MaCrossover::default_params();
        assert!(sig.evaluate(&bars, 52, &iv).is_none());
    }

    #[test]
    fn ema_indicator_keys() {
        let sig = MaCrossover::new(12, 26, MaType::Ema);
        assert_eq!(sig.fast_key, "ema_12");
        assert_eq!(sig.slow_key, "ema_26");
    }

    #[test]
    fn sma_indicator_keys() {
        let sig = MaCrossover::new(10, 50, MaType::Sma);
        assert_eq!(sig.fast_key, "sma_10");
        assert_eq!(sig.slow_key, "sma_50");
    }

    #[test]
    fn name_and_warmup() {
        let sig = MaCrossover::default_params();
        assert_eq!(sig.name(), "ma_crossover");
        assert_eq!(sig.warmup_bars(), 50);
    }

    #[test]
    fn warmup_equals_slow_period() {
        let sig = MaCrossover::new(5, 20, MaType::Ema);
        assert_eq!(sig.warmup_bars(), 20);
    }

    #[test]
    #[should_panic(expected = "slow_period must be > fast_period")]
    fn rejects_slow_leq_fast() {
        MaCrossover::new(50, 10, MaType::Sma);
    }

    #[test]
    #[should_panic(expected = "fast_period must be >= 1")]
    fn rejects_zero_fast_period() {
        MaCrossover::new(0, 10, MaType::Sma);
    }

    #[test]
    fn ema_golden_cross_fires() {
        let bars: Vec<Bar> = (0..35).map(|i| make_bar(i, 100.0, 98.0)).collect();
        let sig = MaCrossover::new(5, 20, MaType::Ema);

        let mut fast = vec![95.0; 35];
        let slow = vec![100.0; 35];
        // At bar 22: fast crosses above slow.
        fast[22] = 105.0;

        let iv = make_indicators_two("ema_5", fast, "ema_20", slow);
        let result = sig.evaluate(&bars, 22, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Long);
    }
}
