//! ROC momentum signal — rate of change exceeds a threshold.
//!
//! Uses the precomputed `roc_{period}` indicator (percent change over N bars).
//! Fires Long when ROC > threshold_pct, Short when ROC < -threshold_pct.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Rate-of-change momentum signal.
///
/// Fires Long when `roc > threshold_pct` and Short when `roc < -threshold_pct`.
/// ROC is expressed as a percentage: `(close[t] / close[t - period] - 1) * 100`.
#[derive(Debug, Clone)]
pub struct RocMomentum {
    pub period: usize,
    pub threshold_pct: f64,
    indicator_key: String,
}

impl RocMomentum {
    pub fn new(period: usize, threshold_pct: f64) -> Self {
        assert!(period >= 1, "period must be >= 1");
        assert!(threshold_pct >= 0.0, "threshold_pct must be >= 0");
        Self {
            period,
            threshold_pct,
            indicator_key: format!("roc_{period}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(12, 0.0)
    }
}

impl SignalGenerator for RocMomentum {
    fn name(&self) -> &str {
        "roc_momentum"
    }

    fn warmup_bars(&self) -> usize {
        self.period
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

        let roc = indicators.get(&self.indicator_key, bar_index)?;
        if roc.is_nan() {
            return None;
        }

        let direction = if roc > self.threshold_pct {
            SignalDirection::Long
        } else if roc < -self.threshold_pct {
            SignalDirection::Short
        } else {
            return None;
        };

        // Strength proportional to how far ROC exceeds threshold
        let excess = roc.abs() - self.threshold_pct;
        let strength = (excess / 10.0).clamp(0.01, 1.0);

        let mut metadata = HashMap::new();
        metadata.insert("roc_value".into(), roc);
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
    fn fires_long_above_threshold() {
        let sig = RocMomentum::new(5, 2.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        for i in 5..10 {
            roc_vals[i] = 5.0; // 5% > 2% threshold
        }
        let iv = make_indicators("roc_5", roc_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Long);
    }

    #[test]
    fn fires_short_below_negative_threshold() {
        let sig = RocMomentum::new(5, 2.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        for i in 5..10 {
            roc_vals[i] = -5.0; // -5% < -2% threshold
        }
        let iv = make_indicators("roc_5", roc_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Short);
    }

    #[test]
    fn no_fire_within_threshold() {
        let sig = RocMomentum::new(5, 2.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        roc_vals[7] = 1.5; // within ±2% threshold
        let iv = make_indicators("roc_5", roc_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn zero_threshold_fires_on_any_nonzero() {
        let sig = RocMomentum::new(5, 0.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        roc_vals[7] = 0.1;
        let iv = make_indicators("roc_5", roc_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_some());
    }

    #[test]
    fn warmup_guard() {
        let sig = RocMomentum::new(5, 0.0);
        let bars = make_bars(10);
        let iv = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 3, &iv).is_none());
    }

    #[test]
    fn nan_guard() {
        let sig = RocMomentum::new(5, 0.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        roc_vals[7] = f64::NAN;
        let iv = make_indicators("roc_5", roc_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn metadata_correctness() {
        let sig = RocMomentum::new(5, 0.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        roc_vals[7] = 3.5;
        let iv = make_indicators("roc_5", roc_vals);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();
        assert_eq!(event.metadata["roc_value"], 3.5);
        assert_eq!(event.metadata["reference_price"], bars[7].close);
        assert_eq!(event.metadata["signal_bar_low"], bars[7].low);
    }

    #[test]
    fn name_and_warmup() {
        let sig = RocMomentum::new(12, 1.0);
        assert_eq!(sig.name(), "roc_momentum");
        assert_eq!(sig.warmup_bars(), 12);
    }

    #[test]
    fn strength_proportional_to_excess() {
        let sig = RocMomentum::new(5, 2.0);
        let bars = make_bars(10);
        let mut roc_vals = vec![f64::NAN; 10];
        roc_vals[7] = 4.0; // excess = 4.0 - 2.0 = 2.0 → strength = 2.0/10.0 = 0.2
        let iv = make_indicators("roc_5", roc_vals);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();
        assert!((event.strength - 0.2).abs() < 1e-10);
    }
}
