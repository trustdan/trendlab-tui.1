//! 52-week breakout signal — price exceeds the N-day high times a threshold.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// 52-week (N-day) breakout signal.
///
/// Fires Long when close > donchian_upper * (1 + threshold_pct / 100).
/// The lookback is typically 252 (one trading year) but is configurable.
#[derive(Debug, Clone)]
pub struct Breakout52w {
    pub lookback: usize,
    pub threshold_pct: f64,
    indicator_key: String,
}

impl Breakout52w {
    pub fn new(lookback: usize, threshold_pct: f64) -> Self {
        assert!(lookback >= 1, "lookback must be >= 1");
        Self {
            lookback,
            threshold_pct,
            indicator_key: format!("donchian_upper_{lookback}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(252, 0.0)
    }
}

impl SignalGenerator for Breakout52w {
    fn name(&self) -> &str {
        "breakout_52w"
    }

    fn warmup_bars(&self) -> usize {
        self.lookback
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

        let threshold = upper * (1.0 + self.threshold_pct / 100.0);
        if bar.close > threshold {
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

    fn make_bars_with_breakout(n: usize, breakout_at: usize) -> Vec<Bar> {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        (0..n)
            .map(|i| {
                let close = if i == breakout_at { 120.0 } else { 100.0 };
                Bar {
                    symbol: "SPY".to_string(),
                    date: base_date + chrono::Duration::days(i as i64),
                    open: close - 0.5,
                    high: if i == breakout_at { 121.0 } else { 102.0 },
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
        let sig = Breakout52w::new(5, 0.0);
        let bars = make_bars_with_breakout(10, 7);
        // donchian_upper_5 = max high over last 5 bars. Before bar 7, highs are 102.
        let mut upper_vals = vec![f64::NAN; 10];
        for i in 4..10 {
            upper_vals[i] = 102.0; // max(high) of preceding 5 bars
        }
        let iv = make_indicators("donchian_upper_5", upper_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.metadata["breakout_level"], 102.0);
    }

    #[test]
    fn no_fire_below_threshold() {
        let sig = Breakout52w::new(5, 0.0);
        let bars = make_bars_with_breakout(10, 7);
        let mut upper_vals = vec![f64::NAN; 10];
        for i in 4..10 {
            upper_vals[i] = 102.0;
        }
        let iv = make_indicators("donchian_upper_5", upper_vals);
        // Bar 6 has close=100, upper=102 → no breakout
        let result = sig.evaluate(&bars, 6, &iv);
        assert!(result.is_none());
    }

    #[test]
    fn warmup_guard() {
        let sig = Breakout52w::new(5, 0.0);
        let bars = make_bars_with_breakout(10, 3);
        let iv = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 3, &iv).is_none());
    }

    #[test]
    fn nan_guard() {
        let sig = Breakout52w::new(5, 0.0);
        let bars = make_bars_with_breakout(10, 7);
        let mut upper_vals = vec![f64::NAN; 10];
        upper_vals[7] = f64::NAN;
        let iv = make_indicators("donchian_upper_5", upper_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn threshold_pct_works() {
        let sig = Breakout52w::new(5, 5.0); // 5% threshold
        let bars = make_bars_with_breakout(10, 7); // bar 7 close = 120
        let mut upper_vals = vec![f64::NAN; 10];
        for i in 4..10 {
            upper_vals[i] = 115.0; // 115 * 1.05 = 120.75 > 120 → no fire
        }
        let iv = make_indicators("donchian_upper_5", upper_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }
}
