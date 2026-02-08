//! Keltner Breakout signal — close exceeds the Keltner upper channel.
//!
//! Fires Long when close > keltner_upper (EMA + multiplier * ATR).
//! The Keltner channel is parameterized by EMA period, ATR period, and multiplier,
//! all of which are encoded in the indicator key.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Keltner channel breakout signal.
///
/// Fires Long when `close > keltner_upper`, where the upper band is
/// `EMA(close, ema_period) + multiplier * ATR(atr_period)`.
///
/// The indicator key encodes all three parameters:
/// `keltner_upper_{ema_period}_{atr_period}_{multiplier}`
#[derive(Debug, Clone)]
pub struct KeltnerBreakout {
    pub ema_period: usize,
    pub atr_period: usize,
    pub multiplier: f64,
    indicator_key: String,
}

impl KeltnerBreakout {
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        assert!(ema_period >= 1, "ema_period must be >= 1");
        assert!(atr_period >= 1, "atr_period must be >= 1");
        assert!(multiplier > 0.0, "multiplier must be > 0");
        Self {
            ema_period,
            atr_period,
            multiplier,
            indicator_key: format!("keltner_upper_{ema_period}_{atr_period}_{multiplier}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(20, 10, 1.5)
    }
}

impl SignalGenerator for KeltnerBreakout {
    fn name(&self) -> &str {
        "keltner_breakout"
    }

    fn warmup_bars(&self) -> usize {
        // Same as the Keltner indicator lookback: max(ema_period - 1, atr_period).
        std::cmp::max(self.ema_period - 1, self.atr_period)
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

        let bar = &bars[bar_index];
        if bar.close.is_nan() {
            return None;
        }

        let upper = indicators.get(&self.indicator_key, bar_index)?;
        if upper.is_nan() {
            return None;
        }

        if bar.close > upper {
            let mut metadata = HashMap::new();
            metadata.insert("breakout_level".into(), upper);
            metadata.insert("reference_price".into(), bar.close);
            metadata.insert("signal_bar_low".into(), bar.low);

            Some(SignalEvent {
                id: SignalEventId(0),
                bar_index,
                date: bar.date,
                symbol: bar.symbol.clone(),
                direction: SignalDirection::Long,
                strength: 1.0,
                metadata,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn base_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
    }

    /// Build `n` bars where bar at `breakout_at` has close=120 (above typical
    /// Keltner upper), and all other bars have close=100.
    fn make_bars(n: usize, breakout_at: Option<usize>) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let close = if breakout_at == Some(i) { 120.0 } else { 100.0 };
                Bar {
                    symbol: "SPY".to_string(),
                    date: base_date() + chrono::Duration::days(i as i64),
                    open: close - 0.5,
                    high: if breakout_at == Some(i) { 121.0 } else { 102.0 },
                    low: close - 2.0,
                    close,
                    volume: 1000,
                    adj_close: close,
                }
            })
            .collect()
    }

    fn make_indicators(key: &str, values: Vec<f64>) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(key.to_string(), values);
        iv
    }

    /// Default key for ema=20, atr=10, mult=1.5.
    fn default_key() -> String {
        "keltner_upper_20_10_1.5".to_string()
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn default_params() {
        let sig = KeltnerBreakout::default_params();
        assert_eq!(sig.ema_period, 20);
        assert_eq!(sig.atr_period, 10);
        assert_eq!(sig.multiplier, 1.5);
        assert_eq!(sig.indicator_key, default_key());
        assert_eq!(sig.name(), "keltner_breakout");
    }

    #[test]
    fn warmup_uses_max_of_ema_minus_one_and_atr() {
        // ema_period=20 -> ema_period-1=19, atr_period=10 -> warmup=19
        let sig = KeltnerBreakout::new(20, 10, 1.5);
        assert_eq!(sig.warmup_bars(), 19);

        // ema_period=5 -> ema_period-1=4, atr_period=14 -> warmup=14
        let sig2 = KeltnerBreakout::new(5, 14, 2.0);
        assert_eq!(sig2.warmup_bars(), 14);

        // ema_period=10 -> ema_period-1=9, atr_period=9 -> warmup=9
        let sig3 = KeltnerBreakout::new(10, 9, 1.0);
        assert_eq!(sig3.warmup_bars(), 9);
    }

    #[test]
    fn indicator_key_encodes_all_params() {
        let sig = KeltnerBreakout::new(30, 14, 2.5);
        assert_eq!(sig.indicator_key, "keltner_upper_30_14_2.5");
    }

    #[test]
    #[should_panic(expected = "ema_period must be >= 1")]
    fn rejects_zero_ema_period() {
        KeltnerBreakout::new(0, 10, 1.5);
    }

    #[test]
    #[should_panic(expected = "atr_period must be >= 1")]
    fn rejects_zero_atr_period() {
        KeltnerBreakout::new(20, 0, 1.5);
    }

    #[test]
    #[should_panic(expected = "multiplier must be > 0")]
    fn rejects_non_positive_multiplier() {
        KeltnerBreakout::new(20, 10, 0.0);
    }

    // -----------------------------------------------------------------------
    // Signal firing
    // -----------------------------------------------------------------------

    #[test]
    fn fires_on_breakout() {
        let sig = KeltnerBreakout::default_params(); // warmup = 19
        let bars = make_bars(25, Some(22)); // bar 22 close = 120
        let mut upper = vec![f64::NAN; 25];
        for i in 19..25 {
            upper[i] = 110.0; // upper band at 110
        }
        let iv = make_indicators(&default_key(), upper);

        let result = sig.evaluate(&bars, 22, &iv);
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.strength, 1.0);
    }

    #[test]
    fn no_fire_below_upper() {
        let sig = KeltnerBreakout::default_params();
        let bars = make_bars(25, None); // all close = 100
        let mut upper = vec![f64::NAN; 25];
        for i in 19..25 {
            upper[i] = 110.0; // upper at 110, close at 100 -> no fire
        }
        let iv = make_indicators(&default_key(), upper);

        assert!(sig.evaluate(&bars, 22, &iv).is_none());
    }

    #[test]
    fn no_fire_at_exact_upper() {
        // close == upper is NOT a breakout (need close > upper)
        let sig = KeltnerBreakout::default_params();
        let bars = make_bars(25, None); // close = 100
        let mut upper = vec![f64::NAN; 25];
        for i in 19..25 {
            upper[i] = 100.0; // upper == close
        }
        let iv = make_indicators(&default_key(), upper);

        assert!(sig.evaluate(&bars, 22, &iv).is_none());
    }

    // -----------------------------------------------------------------------
    // Guards
    // -----------------------------------------------------------------------

    #[test]
    fn warmup_guard() {
        let sig = KeltnerBreakout::default_params(); // warmup = 19
        let bars = make_bars(25, Some(10));
        let iv = IndicatorValues::new();

        // bar_index 10 < warmup 19 -> must return None
        assert!(sig.evaluate(&bars, 10, &iv).is_none());

        // bar_index 18 < warmup 19 -> must return None
        assert!(sig.evaluate(&bars, 18, &iv).is_none());
    }

    #[test]
    fn nan_guard_indicator_nan() {
        let sig = KeltnerBreakout::default_params();
        let bars = make_bars(25, Some(22));
        let mut upper = vec![f64::NAN; 25];
        // Leave bar 22 as NaN
        for i in 19..22 {
            upper[i] = 110.0;
        }
        let iv = make_indicators(&default_key(), upper);

        assert!(sig.evaluate(&bars, 22, &iv).is_none());
    }

    #[test]
    fn nan_guard_indicator_missing() {
        let sig = KeltnerBreakout::default_params();
        let bars = make_bars(25, Some(22));
        let iv = IndicatorValues::new(); // no indicator at all

        assert!(sig.evaluate(&bars, 22, &iv).is_none());
    }

    #[test]
    fn nan_guard_bar_close_nan() {
        let sig = KeltnerBreakout::default_params();
        let mut bars = make_bars(25, Some(22));
        bars[22].close = f64::NAN;
        let mut upper = vec![f64::NAN; 25];
        for i in 19..25 {
            upper[i] = 110.0;
        }
        let iv = make_indicators(&default_key(), upper);

        assert!(sig.evaluate(&bars, 22, &iv).is_none());
    }

    // -----------------------------------------------------------------------
    // Metadata correctness
    // -----------------------------------------------------------------------

    #[test]
    fn metadata_correctness() {
        let sig = KeltnerBreakout::default_params();
        let bars = make_bars(25, Some(22)); // bar 22: close=120, low=118
        let mut upper = vec![f64::NAN; 25];
        for i in 19..25 {
            upper[i] = 110.0;
        }
        let iv = make_indicators(&default_key(), upper);

        let event = sig.evaluate(&bars, 22, &iv).unwrap();
        assert_eq!(event.metadata["breakout_level"], 110.0);
        assert_eq!(event.metadata["reference_price"], 120.0);
        assert_eq!(event.metadata["signal_bar_low"], 118.0); // 120.0 - 2.0
        assert_eq!(event.id, SignalEventId(0));
        assert_eq!(event.bar_index, 22);
        assert_eq!(event.symbol, "SPY");
        assert_eq!(event.date, base_date() + chrono::Duration::days(22));
    }
}
