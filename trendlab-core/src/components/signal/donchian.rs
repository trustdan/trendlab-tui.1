//! Donchian breakout signal — price exceeds the N-day Donchian upper channel.
//!
//! Fires Long when close > donchian_upper (highest high over the lookback window).
//! This is the classic turtle/channel-breakout signal with no threshold buffer.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Donchian channel breakout signal.
///
/// Fires Long when close > donchian_upper (the highest high over the past
/// `entry_lookback` bars). Unlike [`Breakout52w`](super::Breakout52w), this
/// signal has no threshold buffer — any close above the channel triggers.
#[derive(Debug, Clone)]
pub struct DonchianBreakout {
    pub entry_lookback: usize,
    indicator_key: String,
}

impl DonchianBreakout {
    pub fn new(entry_lookback: usize) -> Self {
        assert!(entry_lookback >= 1, "entry_lookback must be >= 1");
        Self {
            entry_lookback,
            indicator_key: format!("donchian_upper_{entry_lookback}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(50)
    }
}

impl SignalGenerator for DonchianBreakout {
    fn name(&self) -> &str {
        "donchian_breakout"
    }

    fn warmup_bars(&self) -> usize {
        self.entry_lookback
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

        let donchian_upper = indicators.get(&self.indicator_key, bar_index)?;
        if donchian_upper.is_nan() {
            return None;
        }

        if bar.close > donchian_upper {
            let mut metadata = HashMap::new();
            metadata.insert("breakout_level".into(), donchian_upper);
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

    fn make_bars(n: usize, breakout_at: Option<usize>) -> Vec<Bar> {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        (0..n)
            .map(|i| {
                let close = if breakout_at == Some(i) { 120.0 } else { 100.0 };
                Bar {
                    symbol: "SPY".to_string(),
                    date: base_date + chrono::Duration::days(i as i64),
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

    #[test]
    fn fires_on_breakout() {
        let sig = DonchianBreakout::new(5);
        let bars = make_bars(10, Some(7));
        // donchian_upper_5: max high of last 5 bars. Before bar 7, highs are 102.
        let mut upper = vec![f64::NAN; 10];
        for i in 5..10 {
            upper[i] = 102.0;
        }
        let iv = make_indicators("donchian_upper_5", upper);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.strength, 1.0);
    }

    #[test]
    fn no_fire_below() {
        let sig = DonchianBreakout::new(5);
        let bars = make_bars(10, Some(7));
        let mut upper = vec![f64::NAN; 10];
        for i in 5..10 {
            upper[i] = 102.0;
        }
        let iv = make_indicators("donchian_upper_5", upper);
        // Bar 6 has close=100, donchian_upper=102 -> no breakout
        let result = sig.evaluate(&bars, 6, &iv);
        assert!(result.is_none());
    }

    #[test]
    fn warmup_guard() {
        let sig = DonchianBreakout::new(5);
        let bars = make_bars(10, Some(3));
        let iv = IndicatorValues::new();
        // bar_index 3 < warmup 5 -> must return None
        assert!(sig.evaluate(&bars, 3, &iv).is_none());
    }

    #[test]
    fn nan_guard() {
        let sig = DonchianBreakout::new(5);
        let bars = make_bars(10, Some(7));

        // Case 1: indicator value is NaN
        let mut upper = vec![f64::NAN; 10];
        upper[7] = f64::NAN;
        let iv = make_indicators("donchian_upper_5", upper);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());

        // Case 2: indicator key missing entirely
        let iv_empty = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 7, &iv_empty).is_none());

        // Case 3: bar.close is NaN
        let mut nan_bars = make_bars(10, Some(7));
        nan_bars[7].close = f64::NAN;
        let mut upper2 = vec![f64::NAN; 10];
        for i in 5..10 {
            upper2[i] = 102.0;
        }
        let iv2 = make_indicators("donchian_upper_5", upper2);
        assert!(sig.evaluate(&nan_bars, 7, &iv2).is_none());
    }

    #[test]
    fn metadata_correctness() {
        let sig = DonchianBreakout::new(5);
        let bars = make_bars(10, Some(7));
        let mut upper = vec![f64::NAN; 10];
        for i in 5..10 {
            upper[i] = 102.0;
        }
        let iv = make_indicators("donchian_upper_5", upper);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();

        assert_eq!(event.metadata["breakout_level"], 102.0);
        assert_eq!(event.metadata["reference_price"], 120.0);
        assert_eq!(event.metadata["signal_bar_low"], 118.0); // 120.0 - 2.0
        assert_eq!(event.id, SignalEventId(0));
        assert_eq!(event.bar_index, 7);
        assert_eq!(event.symbol, "SPY");
        assert_eq!(event.date, NaiveDate::from_ymd_opt(2024, 1, 9).unwrap());
    }

    #[test]
    fn default_params_uses_50() {
        let sig = DonchianBreakout::default_params();
        assert_eq!(sig.entry_lookback, 50);
        assert_eq!(sig.warmup_bars(), 50);
        assert_eq!(sig.name(), "donchian_breakout");
    }

    #[test]
    #[should_panic(expected = "entry_lookback must be >= 1")]
    fn rejects_zero_lookback() {
        DonchianBreakout::new(0);
    }
}
