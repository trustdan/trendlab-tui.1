//! Time-series momentum signal — positive momentum → Long, negative → Short.
//!
//! Uses the precomputed `momentum_{lookback}` indicator.
//! Fires Long when momentum > 0, Short when momentum < 0.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Time-series momentum signal.
///
/// Fires Long when the lookback-period momentum is positive,
/// Short when negative. Momentum = close[t] - close[t - lookback].
#[derive(Debug, Clone)]
pub struct Tsmom {
    pub lookback: usize,
    indicator_key: String,
}

impl Tsmom {
    pub fn new(lookback: usize) -> Self {
        assert!(lookback >= 1, "lookback must be >= 1");
        Self {
            lookback,
            indicator_key: format!("momentum_{lookback}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(20)
    }
}

impl SignalGenerator for Tsmom {
    fn name(&self) -> &str {
        "tsmom"
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

        let momentum = indicators.get(&self.indicator_key, bar_index)?;
        if momentum.is_nan() {
            return None;
        }

        let direction = if momentum > 0.0 {
            SignalDirection::Long
        } else if momentum < 0.0 {
            SignalDirection::Short
        } else {
            return None; // exactly zero — no signal
        };

        let strength = (momentum.abs() / bar.close).min(1.0);

        let mut metadata = HashMap::new();
        metadata.insert("momentum_value".into(), momentum);
        metadata.insert("reference_price".into(), bar.close);
        metadata.insert("signal_bar_low".into(), bar.low);

        Some(SignalEvent {
            id: SignalEventId(0),
            bar_index,
            date: bar.date,
            symbol: bar.symbol.clone(),
            direction,
            strength,
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_bars(n: usize) -> Vec<Bar> {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        (0..n)
            .map(|i| {
                let close = 100.0 + i as f64;
                Bar {
                    symbol: "SPY".to_string(),
                    date: base_date + chrono::Duration::days(i as i64),
                    open: close - 0.5,
                    high: close + 2.0,
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
    fn fires_long_on_positive_momentum() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let mut mom_vals = vec![f64::NAN; 10];
        for i in 5..10 {
            mom_vals[i] = 5.0;
        }
        let iv = make_indicators("momentum_5", mom_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Long);
    }

    #[test]
    fn fires_short_on_negative_momentum() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let mut mom_vals = vec![f64::NAN; 10];
        for i in 5..10 {
            mom_vals[i] = -3.0;
        }
        let iv = make_indicators("momentum_5", mom_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Short);
    }

    #[test]
    fn no_fire_on_zero_momentum() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let mut mom_vals = vec![f64::NAN; 10];
        mom_vals[7] = 0.0;
        let iv = make_indicators("momentum_5", mom_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn warmup_guard() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let iv = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 3, &iv).is_none());
    }

    #[test]
    fn nan_guard() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let mut mom_vals = vec![f64::NAN; 10];
        mom_vals[7] = f64::NAN;
        let iv = make_indicators("momentum_5", mom_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn metadata_correctness() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let mut mom_vals = vec![f64::NAN; 10];
        mom_vals[7] = 10.0;
        let iv = make_indicators("momentum_5", mom_vals);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();
        assert_eq!(event.metadata["momentum_value"], 10.0);
        assert_eq!(event.metadata["reference_price"], bars[7].close);
        assert_eq!(event.metadata["signal_bar_low"], bars[7].low);
    }

    #[test]
    fn strength_capped_at_one() {
        let sig = Tsmom::new(5);
        let bars = make_bars(10);
        let mut mom_vals = vec![f64::NAN; 10];
        mom_vals[7] = 500.0;
        let iv = make_indicators("momentum_5", mom_vals);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();
        assert!(event.strength <= 1.0);
    }

    #[test]
    fn name_and_warmup() {
        let sig = Tsmom::new(20);
        assert_eq!(sig.name(), "tsmom");
        assert_eq!(sig.warmup_bars(), 20);
    }
}
