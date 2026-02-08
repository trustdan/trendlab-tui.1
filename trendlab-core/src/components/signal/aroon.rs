//! Aroon crossover signal - Aroon Up crosses above Aroon Down.
//!
//! Uses precomputed `aroon_up_{period}` and `aroon_down_{period}` indicators.
//! Fires Long when Aroon Up crosses above Aroon Down,
//! Short when Aroon Down crosses above Aroon Up.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Aroon crossover signal.
///
/// Fires Long when Aroon Up crosses above Aroon Down (bullish crossover),
/// Short when Aroon Down crosses above Aroon Up (bearish crossover).
/// Aroon values range from 0 to 100.
#[derive(Debug, Clone)]
pub struct AroonCrossover {
    pub period: usize,
    up_key: String,
    down_key: String,
}

impl AroonCrossover {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "period must be >= 1");
        Self {
            period,
            up_key: format!("aroon_up_{period}"),
            down_key: format!("aroon_down_{period}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(25)
    }
}

impl SignalGenerator for AroonCrossover {
    fn name(&self) -> &str {
        "aroon_crossover"
    }

    fn warmup_bars(&self) -> usize {
        self.period + 1 // need previous bar for crossover detection
    }

    fn evaluate(
        &self,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> Option<SignalEvent> {
        if bar_index < self.warmup_bars() || bar_index == 0 {
            return None;
        }

        let bar = &bars[bar_index];
        if bar.close.is_nan() {
            return None;
        }

        let aroon_up = indicators.get(&self.up_key, bar_index)?;
        let aroon_down = indicators.get(&self.down_key, bar_index)?;
        let prev_up = indicators.get(&self.up_key, bar_index - 1)?;
        let prev_down = indicators.get(&self.down_key, bar_index - 1)?;

        if aroon_up.is_nan() || aroon_down.is_nan() || prev_up.is_nan() || prev_down.is_nan() {
            return None;
        }

        let direction = if aroon_up > aroon_down && prev_up <= prev_down {
            // Bullish crossover: Aroon Up crosses above Aroon Down
            SignalDirection::Long
        } else if aroon_down > aroon_up && prev_down <= prev_up {
            // Bearish crossover: Aroon Down crosses above Aroon Up
            SignalDirection::Short
        } else {
            return None;
        };

        // Strength based on separation between up and down
        let separation = (aroon_up - aroon_down).abs();
        let strength = (separation / 100.0).min(1.0);

        let mut metadata = HashMap::new();
        metadata.insert("aroon_up".into(), aroon_up);
        metadata.insert("aroon_down".into(), aroon_down);
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

    fn make_aroon_indicators(
        period: usize,
        up_vals: Vec<f64>,
        down_vals: Vec<f64>,
    ) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(format!("aroon_up_{period}"), up_vals);
        iv.insert(format!("aroon_down_{period}"), down_vals);
        iv
    }

    #[test]
    fn fires_long_on_bullish_crossover() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let up_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 40.0, 80.0, 90.0, 95.0])
            .collect();
        let down_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 60.0, 40.0, 30.0, 20.0])
            .collect();
        let iv = make_aroon_indicators(5, up_vals, down_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Long);
    }

    #[test]
    fn fires_short_on_bearish_crossover() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let up_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 70.0, 30.0, 20.0, 10.0])
            .collect();
        let down_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 30.0, 80.0, 90.0, 95.0])
            .collect();
        let iv = make_aroon_indicators(5, up_vals, down_vals);
        let result = sig.evaluate(&bars, 7, &iv);
        assert!(result.is_some());
        assert_eq!(result.unwrap().direction, SignalDirection::Short);
    }

    #[test]
    fn no_fire_without_crossover() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let up_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![80.0, 85.0, 90.0, 95.0, 100.0])
            .collect();
        let down_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![20.0, 15.0, 10.0, 5.0, 0.0])
            .collect();
        let iv = make_aroon_indicators(5, up_vals, down_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn warmup_guard() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let iv = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 4, &iv).is_none());
    }

    #[test]
    fn nan_guard() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let up_vals = vec![f64::NAN; 10];
        let down_vals = vec![f64::NAN; 10];
        let iv = make_aroon_indicators(5, up_vals, down_vals);
        assert!(sig.evaluate(&bars, 7, &iv).is_none());
    }

    #[test]
    fn metadata_correctness() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let up_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 40.0, 80.0, 90.0, 95.0])
            .collect();
        let down_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 60.0, 40.0, 30.0, 20.0])
            .collect();
        let iv = make_aroon_indicators(5, up_vals, down_vals);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();
        assert_eq!(event.metadata["aroon_up"], 80.0);
        assert_eq!(event.metadata["aroon_down"], 40.0);
        assert_eq!(event.metadata["reference_price"], bars[7].close);
        assert_eq!(event.metadata["signal_bar_low"], bars[7].low);
    }

    #[test]
    fn name_and_warmup() {
        let sig = AroonCrossover::new(25);
        assert_eq!(sig.name(), "aroon_crossover");
        assert_eq!(sig.warmup_bars(), 26);
    }

    #[test]
    fn strength_proportional_to_separation() {
        let sig = AroonCrossover::new(5);
        let bars = make_bars(10);
        let up_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 40.0, 90.0, 95.0, 100.0])
            .collect();
        let down_vals = vec![f64::NAN; 5]
            .into_iter()
            .chain(vec![50.0, 60.0, 40.0, 30.0, 20.0])
            .collect();
        let iv = make_aroon_indicators(5, up_vals, down_vals);
        let event = sig.evaluate(&bars, 7, &iv).unwrap();
        assert!((event.strength - 0.5).abs() < 1e-10);
    }
}
