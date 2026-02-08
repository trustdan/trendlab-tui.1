//! Parabolic SAR flip signal — detects trend reversals via SAR crossover.
//!
//! Fires Long when SAR flips from above close to below close (bearish-to-bullish).
//! Fires Short when SAR flips from below close to above close (bullish-to-bearish).

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, SignalEventId};

use super::{SignalDirection, SignalEvent, SignalGenerator};
use std::collections::HashMap;

/// Parabolic SAR flip signal generator.
///
/// Detects trend reversals by monitoring the relationship between the SAR value
/// and the bar's close price. A flip from SAR-above-close to SAR-below-close
/// signals a bullish reversal; the inverse signals a bearish reversal.
///
/// # Indicator dependency
/// Requires a precomputed PSAR series keyed as `psar_{af_start}_{af_step}_{af_max}`.
#[derive(Debug, Clone)]
pub struct ParabolicSarSignal {
    pub af_start: f64,
    pub af_step: f64,
    pub af_max: f64,
    indicator_key: String,
}

impl ParabolicSarSignal {
    pub fn new(af_start: f64, af_step: f64, af_max: f64) -> Self {
        assert!(af_start > 0.0, "af_start must be > 0");
        assert!(af_step > 0.0, "af_step must be > 0");
        assert!(af_max > af_start, "af_max must be > af_start");

        let indicator_key = format!("psar_{af_start}_{af_step}_{af_max}");
        Self {
            af_start,
            af_step,
            af_max,
            indicator_key,
        }
    }

    pub fn default_params() -> Self {
        Self::new(0.02, 0.02, 0.20)
    }
}

impl SignalGenerator for ParabolicSarSignal {
    fn name(&self) -> &str {
        "parabolic_sar"
    }

    fn warmup_bars(&self) -> usize {
        2
    }

    fn evaluate(
        &self,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> Option<SignalEvent> {
        // Guard: need at least 2 bars for flip detection.
        if bar_index == 0 || bar_index < self.warmup_bars() {
            return None;
        }

        let bar = &bars[bar_index];
        let prev_bar = &bars[bar_index - 1];

        // NaN guard: current bar close.
        if bar.close.is_nan() {
            return None;
        }
        // NaN guard: previous bar close.
        if prev_bar.close.is_nan() {
            return None;
        }

        // Fetch SAR values.
        let sar = indicators.get(&self.indicator_key, bar_index)?;
        let sar_prev = indicators.get(&self.indicator_key, bar_index - 1)?;

        // NaN guard: indicator values.
        if sar.is_nan() || sar_prev.is_nan() {
            return None;
        }

        let mut metadata = HashMap::new();
        metadata.insert("breakout_level".into(), sar);
        metadata.insert("reference_price".into(), bar.close);
        metadata.insert("signal_bar_low".into(), bar.low);

        // Long signal: SAR flips from above to below close.
        // Current bar: SAR < close. Previous bar: SAR >= close.
        if sar < bar.close && sar_prev >= prev_bar.close {
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

        // Short signal: SAR flips from below to above close.
        // Current bar: SAR > close. Previous bar: SAR <= close.
        if sar > bar.close && sar_prev <= prev_bar.close {
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
            symbol: "SPY".to_string(),
            date: base_date() + chrono::Duration::days(index as i64),
            open: close - 0.5,
            high: close + 1.0,
            low,
            close,
            volume: 1000,
            adj_close: close,
        }
    }

    fn make_indicators(key: &str, values: Vec<f64>) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(key.to_string(), values);
        iv
    }

    #[test]
    fn fires_long_on_sar_flip_below() {
        // Bar 0: SAR above close (bearish). Bar 1: SAR below close (bullish flip).
        let bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        // SAR: [110, 108, 99] — at bar 1, sar(108) >= close(102); at bar 2, sar(99) < close(105)
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0, 108.0, 99.0]);

        let sig = ParabolicSarSignal::default_params();
        let result = sig.evaluate(&bars, 2, &iv);
        assert!(result.is_some(), "expected Long signal on SAR flip below");
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Long);
        assert_eq!(event.bar_index, 2);
        assert_eq!(event.metadata["breakout_level"], 99.0);
        assert_eq!(event.metadata["reference_price"], 105.0);
        assert_eq!(event.metadata["signal_bar_low"], 103.0);
    }

    #[test]
    fn fires_short_on_sar_flip_above() {
        // Bar 1: SAR below close (bullish). Bar 2: SAR above close (bearish flip).
        let bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 105.0, 103.0),
            make_bar(2, 98.0, 96.0),
        ];
        // SAR: [95, 97, 106] — at bar 1, sar(97) <= close(105); at bar 2, sar(106) > close(98)
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![95.0, 97.0, 106.0]);

        let sig = ParabolicSarSignal::default_params();
        let result = sig.evaluate(&bars, 2, &iv);
        assert!(result.is_some(), "expected Short signal on SAR flip above");
        let event = result.unwrap();
        assert_eq!(event.direction, SignalDirection::Short);
        assert_eq!(event.bar_index, 2);
        assert_eq!(event.metadata["breakout_level"], 106.0);
    }

    #[test]
    fn no_fire_when_trend_continues() {
        // Both bars: SAR below close (bullish continuation, no flip).
        let bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        // SAR: [90, 92, 95] — SAR stays below close throughout.
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![90.0, 92.0, 95.0]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 2, &iv).is_none());
    }

    #[test]
    fn warmup_guard_bar_zero() {
        let bars = vec![make_bar(0, 100.0, 98.0)];
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 0, &iv).is_none());
    }

    #[test]
    fn warmup_guard_bar_one() {
        // warmup_bars() == 2, so bar_index 1 should not fire.
        let bars = vec![make_bar(0, 100.0, 98.0), make_bar(1, 105.0, 103.0)];
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0, 99.0]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 1, &iv).is_none());
    }

    #[test]
    fn nan_guard_current_bar_close() {
        let mut bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        bars[2].close = f64::NAN;
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0, 108.0, 99.0]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 2, &iv).is_none());
    }

    #[test]
    fn nan_guard_previous_bar_close() {
        let mut bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        bars[1].close = f64::NAN;
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0, 108.0, 99.0]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 2, &iv).is_none());
    }

    #[test]
    fn nan_guard_current_indicator() {
        let bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0, 108.0, f64::NAN]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 2, &iv).is_none());
    }

    #[test]
    fn nan_guard_previous_indicator() {
        let bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        let iv = make_indicators("psar_0.02_0.02_0.2", vec![110.0, f64::NAN, 99.0]);

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 2, &iv).is_none());
    }

    #[test]
    fn missing_indicator_returns_none() {
        let bars = vec![
            make_bar(0, 100.0, 98.0),
            make_bar(1, 102.0, 100.0),
            make_bar(2, 105.0, 103.0),
        ];
        // No indicators inserted at all.
        let iv = IndicatorValues::new();

        let sig = ParabolicSarSignal::default_params();
        assert!(sig.evaluate(&bars, 2, &iv).is_none());
    }

    #[test]
    fn indicator_key_format() {
        let sig = ParabolicSarSignal::new(0.02, 0.02, 0.2);
        assert_eq!(sig.indicator_key, "psar_0.02_0.02_0.2");
    }

    #[test]
    fn name_and_warmup() {
        let sig = ParabolicSarSignal::default_params();
        assert_eq!(sig.name(), "parabolic_sar");
        assert_eq!(sig.warmup_bars(), 2);
    }

    #[test]
    #[should_panic(expected = "af_start must be > 0")]
    fn rejects_zero_af_start() {
        ParabolicSarSignal::new(0.0, 0.02, 0.20);
    }

    #[test]
    #[should_panic(expected = "af_max must be > af_start")]
    fn rejects_af_max_leq_af_start() {
        ParabolicSarSignal::new(0.20, 0.02, 0.20);
    }
}
