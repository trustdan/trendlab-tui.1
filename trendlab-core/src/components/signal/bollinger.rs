//! Bollinger breakout signal — price exceeds the upper Bollinger Band.
//!
//! Fires Long when close > bollinger_upper (SMA + std_multiplier * stdev over
//! the lookback `period`). This is a volatility-adjusted breakout signal.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Bollinger Band upper-channel breakout signal.
///
/// Fires Long when close > bollinger_upper (the SMA plus `std_multiplier`
/// standard deviations over the past `period` bars).
#[derive(Debug, Clone)]
pub struct BollingerBreakout {
    pub period: usize,
    pub std_multiplier: f64,
    indicator_key: String,
}

impl BollingerBreakout {
    pub fn new(period: usize, std_multiplier: f64) -> Self {
        assert!(period >= 1, "period must be >= 1");
        assert!(
            std_multiplier > 0.0 && std_multiplier.is_finite(),
            "std_multiplier must be positive and finite"
        );
        Self {
            period,
            std_multiplier,
            indicator_key: format!("bollinger_upper_{period}_{std_multiplier}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(20, 2.0)
    }
}

impl SignalGenerator for BollingerBreakout {
    fn name(&self) -> &str {
        "bollinger_breakout"
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

        let bollinger_upper = indicators.get(&self.indicator_key, bar_index)?;
        if bollinger_upper.is_nan() {
            return None;
        }

        if bar.close > bollinger_upper {
            let mut metadata = HashMap::new();
            metadata.insert("breakout_level".into(), bollinger_upper);
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

    /// The default indicator key for period=20, multiplier=2.0
    const DEFAULT_KEY: &str = "bollinger_upper_20_2";

    #[test]
    fn fires_on_breakout() {
        let sig = BollingerBreakout::default_params();
        let bars = make_bars(30, Some(25));
        // bollinger_upper_20_2: SMA(20) + 2*stdev. Set to 110 so close=120 > 110.
        let mut upper = vec![f64::NAN; 30];
        for i in 20..30 {
            upper[i] = 110.0;
        }
        let iv = make_indicators(DEFAULT_KEY, upper);
        let result = sig.evaluate(&bars, 25, &iv);
        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.strength, 1.0);
    }

    #[test]
    fn no_fire_below() {
        let sig = BollingerBreakout::default_params();
        let bars = make_bars(30, Some(25));
        let mut upper = vec![f64::NAN; 30];
        for i in 20..30 {
            upper[i] = 110.0;
        }
        let iv = make_indicators(DEFAULT_KEY, upper);
        // Bar 24 has close=100, bollinger_upper=110 -> no breakout
        let result = sig.evaluate(&bars, 24, &iv);
        assert!(result.is_none());
    }

    #[test]
    fn warmup_guard() {
        let sig = BollingerBreakout::default_params();
        let bars = make_bars(30, Some(10));
        let iv = IndicatorValues::new();
        // bar_index 10 < warmup 20 -> must return None
        assert!(sig.evaluate(&bars, 10, &iv).is_none());
    }

    #[test]
    fn nan_guard() {
        let sig = BollingerBreakout::default_params();
        let bars = make_bars(30, Some(25));

        // Case 1: indicator value is NaN
        let mut upper = vec![f64::NAN; 30];
        upper[25] = f64::NAN;
        let iv = make_indicators(DEFAULT_KEY, upper);
        assert!(sig.evaluate(&bars, 25, &iv).is_none());

        // Case 2: indicator key missing entirely
        let iv_empty = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 25, &iv_empty).is_none());

        // Case 3: bar.close is NaN
        let mut nan_bars = make_bars(30, Some(25));
        nan_bars[25].close = f64::NAN;
        let mut upper2 = vec![f64::NAN; 30];
        for i in 20..30 {
            upper2[i] = 110.0;
        }
        let iv2 = make_indicators(DEFAULT_KEY, upper2);
        assert!(sig.evaluate(&nan_bars, 25, &iv2).is_none());
    }

    #[test]
    fn metadata_correctness() {
        let sig = BollingerBreakout::default_params();
        let bars = make_bars(30, Some(25));
        let mut upper = vec![f64::NAN; 30];
        for i in 20..30 {
            upper[i] = 110.0;
        }
        let iv = make_indicators(DEFAULT_KEY, upper);
        let event = sig.evaluate(&bars, 25, &iv).unwrap();

        assert_eq!(event.metadata["breakout_level"], 110.0);
        assert_eq!(event.metadata["reference_price"], 120.0);
        assert_eq!(event.metadata["signal_bar_low"], 118.0); // 120.0 - 2.0
        assert_eq!(event.id, SignalEventId(0));
        assert_eq!(event.bar_index, 25);
        assert_eq!(event.symbol, "SPY");
        assert_eq!(event.date, NaiveDate::from_ymd_opt(2024, 1, 27).unwrap());
    }

    #[test]
    fn custom_params_indicator_key() {
        let sig = BollingerBreakout::new(30, 2.5);
        assert_eq!(sig.period, 30);
        assert_eq!(sig.std_multiplier, 2.5);
        assert_eq!(sig.warmup_bars(), 30);

        // Verify the indicator key includes both period and multiplier
        let bars = make_bars(40, Some(35));
        let mut upper = vec![f64::NAN; 40];
        for i in 30..40 {
            upper[i] = 105.0;
        }
        let iv = make_indicators("bollinger_upper_30_2.5", upper);
        let result = sig.evaluate(&bars, 35, &iv);
        assert!(result.is_some());
    }

    #[test]
    fn default_params_uses_20_2() {
        let sig = BollingerBreakout::default_params();
        assert_eq!(sig.period, 20);
        assert_eq!(sig.std_multiplier, 2.0);
        assert_eq!(sig.warmup_bars(), 20);
        assert_eq!(sig.name(), "bollinger_breakout");
    }

    #[test]
    #[should_panic(expected = "period must be >= 1")]
    fn rejects_zero_period() {
        BollingerBreakout::new(0, 2.0);
    }

    #[test]
    #[should_panic(expected = "std_multiplier must be positive and finite")]
    fn rejects_negative_multiplier() {
        BollingerBreakout::new(20, -1.0);
    }

    #[test]
    #[should_panic(expected = "std_multiplier must be positive and finite")]
    fn rejects_nan_multiplier() {
        BollingerBreakout::new(20, f64::NAN);
    }
}
