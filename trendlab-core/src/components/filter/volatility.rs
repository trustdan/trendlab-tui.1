//! Volatility signal filter - gates signals by ATR-based volatility level.
//!
//! Passes signals when volatility (ATR as % of price) is within
//! the specified range. Rejects in extremely low-vol (no movement)
//! or extremely high-vol (erratic) environments.

use crate::components::indicator::IndicatorValues;
use crate::components::signal::{FilterVerdict, SignalEvaluation, SignalEvent};
use crate::domain::Bar;
use std::collections::HashMap;

use super::SignalFilter;

/// ATR-based volatility filter.
///
/// Computes `volatility_pct = (atr / close) * 100` and passes signals
/// when the value falls within `[min_pct, max_pct]`.
#[derive(Debug, Clone)]
pub struct VolatilityFilter {
    pub period: usize,
    pub min_pct: f64,
    pub max_pct: f64,
    indicator_key: String,
}

impl VolatilityFilter {
    pub fn new(period: usize, min_pct: f64, max_pct: f64) -> Self {
        assert!(period >= 1, "period must be >= 1");
        assert!(min_pct >= 0.0, "min_pct must be >= 0");
        assert!(max_pct >= min_pct, "max_pct must be >= min_pct");
        Self {
            period,
            min_pct,
            max_pct,
            indicator_key: format!("atr_{period}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(14, 0.5, 5.0)
    }
}

impl SignalFilter for VolatilityFilter {
    fn name(&self) -> &str {
        "volatility_filter"
    }

    fn evaluate(
        &self,
        signal: &SignalEvent,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> SignalEvaluation {
        let close = if bar_index < bars.len() {
            bars[bar_index].close
        } else {
            f64::NAN
        };

        let atr_value = indicators.get(&self.indicator_key, bar_index);

        let (verdict, filter_state) = match atr_value {
            Some(atr) if !atr.is_nan() && !close.is_nan() && close > 0.0 => {
                let vol_pct = (atr / close) * 100.0;
                let mut state = HashMap::new();
                state.insert("atr_value".into(), atr);
                state.insert("close".into(), close);
                state.insert("volatility_pct".into(), vol_pct);
                if vol_pct >= self.min_pct && vol_pct <= self.max_pct {
                    (FilterVerdict::Passed, state)
                } else {
                    (FilterVerdict::FilteredByVolatility, state)
                }
            }
            _ => (FilterVerdict::FilteredByVolatility, HashMap::new()),
        };

        SignalEvaluation {
            signal_event_id: signal.id,
            filter_name: self.name().to_string(),
            verdict,
            filter_state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::signal::SignalDirection;
    use crate::domain::SignalEventId;
    use chrono::NaiveDate;

    fn make_signal() -> SignalEvent {
        SignalEvent {
            id: SignalEventId(1),
            bar_index: 5,
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "SPY".into(),
            direction: SignalDirection::Long,
            strength: 0.8,
            metadata: HashMap::new(),
        }
    }

    fn make_bars_with_close(closes: &[f64]) -> Vec<Bar> {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| Bar {
                symbol: "SPY".to_string(),
                date: base_date + chrono::Duration::days(i as i64),
                open: close - 0.5,
                high: close + 2.0,
                low: close - 2.0,
                close,
                volume: 1000,
                adj_close: close,
            })
            .collect()
    }

    fn make_indicators(key: &str, values: Vec<f64>) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(key.to_string(), values);
        iv
    }

    #[test]
    fn passes_within_range() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        // atr=2.0, close=100.0 -> vol_pct=2.0% (within 0.5-5.0)
        let mut atr_vals = vec![f64::NAN; 10];
        atr_vals[5] = 2.0;
        let iv = make_indicators("atr_14", atr_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(eval.verdict.is_passed());
    }

    #[test]
    fn rejects_too_low_vol() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        // atr=0.1, close=100.0 -> vol_pct=0.1% (below 0.5)
        let mut atr_vals = vec![f64::NAN; 10];
        atr_vals[5] = 0.1;
        let iv = make_indicators("atr_14", atr_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
        assert_eq!(eval.verdict, FilterVerdict::FilteredByVolatility);
    }

    #[test]
    fn rejects_too_high_vol() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        // atr=10.0, close=100.0 -> vol_pct=10.0% (above 5.0)
        let mut atr_vals = vec![f64::NAN; 10];
        atr_vals[5] = 10.0;
        let iv = make_indicators("atr_14", atr_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
        assert_eq!(eval.verdict, FilterVerdict::FilteredByVolatility);
    }

    #[test]
    fn passes_at_boundaries() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        // Exactly at min: atr=0.5 -> vol_pct=0.5%
        let mut atr_vals = vec![f64::NAN; 10];
        atr_vals[5] = 0.5;
        let iv = make_indicators("atr_14", atr_vals);
        assert!(filter.evaluate(&signal, &bars, 5, &iv).verdict.is_passed());

        // Exactly at max: atr=5.0 -> vol_pct=5.0%
        let mut atr_vals2 = vec![f64::NAN; 10];
        atr_vals2[5] = 5.0;
        let iv2 = make_indicators("atr_14", atr_vals2);
        assert!(filter.evaluate(&signal, &bars, 5, &iv2).verdict.is_passed());
    }

    #[test]
    fn nan_guard() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let atr_vals = vec![f64::NAN; 10];
        let iv = make_indicators("atr_14", atr_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn missing_indicator_rejects() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let iv = IndicatorValues::new();
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn filter_state_snapshot() {
        let filter = VolatilityFilter::new(14, 0.5, 5.0);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let mut atr_vals = vec![f64::NAN; 10];
        atr_vals[5] = 2.0;
        let iv = make_indicators("atr_14", atr_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert_eq!(eval.filter_state["atr_value"], 2.0);
        assert_eq!(eval.filter_state["close"], 100.0);
        assert_eq!(eval.filter_state["volatility_pct"], 2.0);
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(
            VolatilityFilter::default_params().name(),
            "volatility_filter"
        );
    }
}
