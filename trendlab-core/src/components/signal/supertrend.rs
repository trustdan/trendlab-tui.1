//! Supertrend direction flip signal — detects trend reversals.
//!
//! The Supertrend indicator outputs a single band value that alternates between
//! acting as support (below price = uptrend) and resistance (above price = downtrend).
//!
//! This signal fires on the bar where the relationship between close and supertrend
//! flips direction:
//! - Long: supertrend transitions from above close to below close (downtrend -> uptrend)
//! - Short: supertrend transitions from below close to above close (uptrend -> downtrend)

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Supertrend direction flip signal.
///
/// Detects crossovers between the Supertrend band and close price.
/// Requires two consecutive bars to confirm the flip — bar_index-1 is the
/// "before" state and bar_index is the "after" state.
///
/// The indicator key encodes both parameters:
/// `supertrend_{period}_{multiplier}`
#[derive(Debug, Clone)]
pub struct SupertrendSignal {
    pub period: usize,
    pub multiplier: f64,
    indicator_key: String,
}

impl SupertrendSignal {
    pub fn new(period: usize, multiplier: f64) -> Self {
        assert!(period >= 1, "period must be >= 1");
        assert!(multiplier > 0.0, "multiplier must be > 0");
        Self {
            period,
            multiplier,
            indicator_key: format!("supertrend_{period}_{multiplier}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(10, 3.0)
    }
}

impl SignalGenerator for SupertrendSignal {
    fn name(&self) -> &str {
        "supertrend_flip"
    }

    fn warmup_bars(&self) -> usize {
        // Indicator lookback (period) + 1 for flip detection (need previous bar).
        self.period + 1
    }

    fn evaluate(
        &self,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> Option<SignalEvent> {
        if bar_index < self.warmup_bars() {
            return None;
        }

        // Need current and previous bar.
        if bar_index == 0 {
            return None;
        }

        let bar_cur = &bars[bar_index];
        let bar_prev = &bars[bar_index - 1];

        // NaN guard: both bars' close values must be valid.
        if bar_cur.close.is_nan() || bar_prev.close.is_nan() {
            return None;
        }

        // Fetch supertrend values for current and previous bars.
        let st_cur = indicators.get(&self.indicator_key, bar_index)?;
        let st_prev = indicators.get(&self.indicator_key, bar_index - 1)?;

        // NaN guard: both supertrend values must be valid.
        if st_cur.is_nan() || st_prev.is_nan() {
            return None;
        }

        let close_cur = bar_cur.close;
        let close_prev = bar_prev.close;

        // Detect direction flip.
        let direction = if st_cur < close_cur && st_prev >= close_prev {
            // Previous: supertrend >= close (downtrend / resistance)
            // Current:  supertrend <  close (uptrend / support)
            // Flip from down to up -> Long
            Some(SignalDirection::Long)
        } else if st_cur > close_cur && st_prev <= close_prev {
            // Previous: supertrend <= close (uptrend / support)
            // Current:  supertrend >  close (downtrend / resistance)
            // Flip from up to down -> Short
            Some(SignalDirection::Short)
        } else {
            None
        };

        direction.map(|dir| {
            let mut metadata = HashMap::new();
            metadata.insert("breakout_level".into(), st_cur);
            metadata.insert("reference_price".into(), close_cur);
            metadata.insert("signal_bar_low".into(), bar_cur.low);

            SignalEvent {
                id: SignalEventId(0),
                bar_index,
                date: bar_cur.date,
                symbol: bar_cur.symbol.clone(),
                direction: dir,
                strength: 1.0,
                metadata,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn base_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
    }

    /// Build `n` bars with customizable close prices per bar.
    fn make_bars_with_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| Bar {
                symbol: "AAPL".to_string(),
                date: base_date() + chrono::Duration::days(i as i64),
                open: close - 0.5,
                high: close + 1.0,
                low: close - 2.0,
                close,
                volume: 5000,
                adj_close: close,
            })
            .collect()
    }

    fn make_indicators(key: &str, values: Vec<f64>) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(key.to_string(), values);
        iv
    }

    /// Default key for period=10, multiplier=3.0.
    fn default_key() -> String {
        "supertrend_10_3".to_string()
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn default_params() {
        let sig = SupertrendSignal::default_params();
        assert_eq!(sig.period, 10);
        assert_eq!(sig.multiplier, 3.0);
        assert_eq!(sig.indicator_key, default_key());
        assert_eq!(sig.name(), "supertrend_flip");
    }

    #[test]
    fn warmup_is_period_plus_one() {
        let sig = SupertrendSignal::new(10, 3.0);
        assert_eq!(sig.warmup_bars(), 11);

        let sig2 = SupertrendSignal::new(20, 2.0);
        assert_eq!(sig2.warmup_bars(), 21);
    }

    #[test]
    fn indicator_key_encodes_params() {
        let sig = SupertrendSignal::new(14, 2.5);
        assert_eq!(sig.indicator_key, "supertrend_14_2.5");
    }

    #[test]
    #[should_panic(expected = "period must be >= 1")]
    fn rejects_zero_period() {
        SupertrendSignal::new(0, 3.0);
    }

    #[test]
    #[should_panic(expected = "multiplier must be > 0")]
    fn rejects_non_positive_multiplier() {
        SupertrendSignal::new(10, 0.0);
    }

    // -----------------------------------------------------------------------
    // Long flip (downtrend -> uptrend)
    // -----------------------------------------------------------------------

    #[test]
    fn fires_long_on_flip_from_down_to_up() {
        let sig = SupertrendSignal::default_params(); // warmup = 11
                                                      // 15 bars, close steady at 100.
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        // Supertrend values: bars 0..10 are NaN (warmup).
        // Bar 11: supertrend=105 (above close=100 -> downtrend)
        // Bar 12: supertrend=95  (below close=100 -> uptrend) => FLIP -> Long
        let mut st = vec![f64::NAN; 15];
        st[10] = 105.0;
        st[11] = 105.0; // prev: st >= close (downtrend)
        st[12] = 95.0; // cur: st < close (uptrend) => flip
        st[13] = 93.0;
        st[14] = 91.0;
        let iv = make_indicators(&default_key(), st);

        let result = sig.evaluate(&bars, 12, &iv);
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.strength, 1.0);
        assert_eq!(event.symbol, "AAPL");
    }

    // -----------------------------------------------------------------------
    // Short flip (uptrend -> downtrend)
    // -----------------------------------------------------------------------

    #[test]
    fn fires_short_on_flip_from_up_to_down() {
        let sig = SupertrendSignal::default_params(); // warmup = 11
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        // Bar 11: supertrend=95 (below close=100 -> uptrend)
        // Bar 12: supertrend=105 (above close=100 -> downtrend) => FLIP -> Short
        let mut st = vec![f64::NAN; 15];
        st[10] = 95.0;
        st[11] = 95.0; // prev: st <= close (uptrend)
        st[12] = 105.0; // cur: st > close (downtrend) => flip
        st[13] = 107.0;
        st[14] = 109.0;
        let iv = make_indicators(&default_key(), st);

        let result = sig.evaluate(&bars, 12, &iv);
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Short);
        assert_eq!(event.strength, 1.0);
    }

    // -----------------------------------------------------------------------
    // No fire cases
    // -----------------------------------------------------------------------

    #[test]
    fn no_fire_when_already_in_uptrend() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        // Both bars: supertrend < close (uptrend, no flip)
        let mut st = vec![f64::NAN; 15];
        st[11] = 95.0; // prev: uptrend
        st[12] = 93.0; // cur: still uptrend
        let iv = make_indicators(&default_key(), st);

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    #[test]
    fn no_fire_when_already_in_downtrend() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        // Both bars: supertrend > close (downtrend, no flip)
        let mut st = vec![f64::NAN; 15];
        st[11] = 105.0; // prev: downtrend
        st[12] = 107.0; // cur: still downtrend
        let iv = make_indicators(&default_key(), st);

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    // -----------------------------------------------------------------------
    // Edge: exact equality
    // -----------------------------------------------------------------------

    #[test]
    fn long_flip_with_exact_equality_on_previous_bar() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        // Bar 11: supertrend == close (100 >= 100, treated as downtrend)
        // Bar 12: supertrend < close (95 < 100, uptrend) => Long flip
        let mut st = vec![f64::NAN; 15];
        st[11] = 100.0; // st_prev == close_prev -> st_prev >= close_prev is true
        st[12] = 95.0; // st_cur < close_cur
        let iv = make_indicators(&default_key(), st);

        let result = sig.evaluate(&bars, 12, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Long);
    }

    #[test]
    fn short_flip_with_exact_equality_on_previous_bar() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        // Bar 11: supertrend == close (100 <= 100, treated as uptrend)
        // Bar 12: supertrend > close (105 > 100, downtrend) => Short flip
        let mut st = vec![f64::NAN; 15];
        st[11] = 100.0; // st_prev == close_prev -> st_prev <= close_prev is true
        st[12] = 105.0; // st_cur > close_cur
        let iv = make_indicators(&default_key(), st);

        let result = sig.evaluate(&bars, 12, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Short);
    }

    // -----------------------------------------------------------------------
    // Guards
    // -----------------------------------------------------------------------

    #[test]
    fn warmup_guard() {
        let sig = SupertrendSignal::default_params(); // warmup = 11
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);
        let iv = IndicatorValues::new();

        // bar_index 10 < warmup 11 -> must return None
        assert!(sig.evaluate(&bars, 10, &iv).is_none());

        // bar_index 0 -> None (also caught by warmup guard)
        assert!(sig.evaluate(&bars, 0, &iv).is_none());
    }

    #[test]
    fn nan_guard_current_bar_close() {
        let sig = SupertrendSignal::default_params();
        let mut closes = vec![100.0; 15];
        closes[12] = f64::NAN;
        let bars = make_bars_with_closes(&closes);

        let mut st = vec![f64::NAN; 15];
        st[11] = 105.0;
        st[12] = 95.0;
        let iv = make_indicators(&default_key(), st);

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    #[test]
    fn nan_guard_previous_bar_close() {
        let sig = SupertrendSignal::default_params();
        let mut closes = vec![100.0; 15];
        closes[11] = f64::NAN;
        let bars = make_bars_with_closes(&closes);

        let mut st = vec![f64::NAN; 15];
        st[11] = 105.0;
        st[12] = 95.0;
        let iv = make_indicators(&default_key(), st);

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    #[test]
    fn nan_guard_current_supertrend() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        let mut st = vec![f64::NAN; 15];
        st[11] = 105.0;
        // st[12] remains NaN
        let iv = make_indicators(&default_key(), st);

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    #[test]
    fn nan_guard_previous_supertrend() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        let mut st = vec![f64::NAN; 15];
        // st[11] remains NaN
        st[12] = 95.0;
        let iv = make_indicators(&default_key(), st);

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    #[test]
    fn nan_guard_indicator_missing_entirely() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);
        let iv = IndicatorValues::new();

        assert!(sig.evaluate(&bars, 12, &iv).is_none());
    }

    // -----------------------------------------------------------------------
    // Metadata correctness
    // -----------------------------------------------------------------------

    #[test]
    fn metadata_correctness_long() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        let mut st = vec![f64::NAN; 15];
        st[11] = 105.0; // prev: downtrend
        st[12] = 95.0; // cur: uptrend -> Long flip
        let iv = make_indicators(&default_key(), st);

        let event = sig.evaluate(&bars, 12, &iv).unwrap();
        assert_eq!(event.metadata["breakout_level"], 95.0); // current supertrend
        assert_eq!(event.metadata["reference_price"], 100.0); // current close
        assert_eq!(event.metadata["signal_bar_low"], 98.0); // 100.0 - 2.0
        assert_eq!(event.id, SignalEventId(0));
        assert_eq!(event.bar_index, 12);
        assert_eq!(event.symbol, "AAPL");
        assert_eq!(event.date, base_date() + chrono::Duration::days(12));
    }

    #[test]
    fn metadata_correctness_short() {
        let sig = SupertrendSignal::default_params();
        let closes = vec![100.0; 15];
        let bars = make_bars_with_closes(&closes);

        let mut st = vec![f64::NAN; 15];
        st[11] = 95.0; // prev: uptrend
        st[12] = 105.0; // cur: downtrend -> Short flip
        let iv = make_indicators(&default_key(), st);

        let event = sig.evaluate(&bars, 12, &iv).unwrap();
        assert_eq!(event.metadata["breakout_level"], 105.0); // current supertrend
        assert_eq!(event.metadata["reference_price"], 100.0);
        assert_eq!(event.metadata["signal_bar_low"], 98.0);
        assert_eq!(event.direction, SignalDirection::Short);
    }
}
