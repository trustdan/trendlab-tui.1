//! ADX signal filter - gates signals by trend strength.
//!
//! Passes signals when ADX >= threshold (indicating a trending market).
//! Rejects signals in low-ADX (range-bound) environments.

use crate::components::indicator::IndicatorValues;
use crate::components::signal::{FilterVerdict, SignalEvaluation, SignalEvent};
use crate::domain::Bar;
use std::collections::HashMap;

use super::SignalFilter;

/// ADX trend strength filter.
///
/// Passes signals when ADX is at or above the threshold,
/// indicating sufficient trend strength for a breakout/trend-following system.
#[derive(Debug, Clone)]
pub struct AdxFilter {
    pub period: usize,
    pub threshold: f64,
    indicator_key: String,
}

impl AdxFilter {
    pub fn new(period: usize, threshold: f64) -> Self {
        assert!(period >= 1, "period must be >= 1");
        assert!(threshold >= 0.0, "threshold must be >= 0");
        Self {
            period,
            threshold,
            indicator_key: format!("adx_{period}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(14, 25.0)
    }
}

impl SignalFilter for AdxFilter {
    fn name(&self) -> &str {
        "adx_filter"
    }

    fn evaluate(
        &self,
        signal: &SignalEvent,
        _bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> SignalEvaluation {
        let adx_value = indicators.get(&self.indicator_key, bar_index);

        let (verdict, filter_state) = match adx_value {
            Some(adx) if !adx.is_nan() => {
                let mut state = HashMap::new();
                state.insert("adx_value".into(), adx);
                state.insert("threshold".into(), self.threshold);
                if adx >= self.threshold {
                    (FilterVerdict::Passed, state)
                } else {
                    (FilterVerdict::FilteredByAdx, state)
                }
            }
            _ => {
                // NaN or missing indicator -> reject
                (FilterVerdict::FilteredByAdx, HashMap::new())
            }
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
            bar_index: 10,
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "SPY".into(),
            direction: SignalDirection::Long,
            strength: 0.8,
            metadata: HashMap::new(),
        }
    }

    fn make_indicators(key: &str, values: Vec<f64>) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        iv.insert(key.to_string(), values);
        iv
    }

    #[test]
    fn passes_when_adx_above_threshold() {
        let filter = AdxFilter::new(14, 25.0);
        let signal = make_signal();
        let mut adx_vals = vec![f64::NAN; 15];
        adx_vals[10] = 30.0; // above threshold
        let iv = make_indicators("adx_14", adx_vals);
        let eval = filter.evaluate(&signal, &[], 10, &iv);
        assert!(eval.verdict.is_passed());
        assert_eq!(eval.filter_state["adx_value"], 30.0);
    }

    #[test]
    fn rejects_when_adx_below_threshold() {
        let filter = AdxFilter::new(14, 25.0);
        let signal = make_signal();
        let mut adx_vals = vec![f64::NAN; 15];
        adx_vals[10] = 18.0; // below threshold
        let iv = make_indicators("adx_14", adx_vals);
        let eval = filter.evaluate(&signal, &[], 10, &iv);
        assert!(!eval.verdict.is_passed());
        assert_eq!(eval.verdict, FilterVerdict::FilteredByAdx);
        assert_eq!(eval.filter_state["adx_value"], 18.0);
    }

    #[test]
    fn passes_at_exact_threshold() {
        let filter = AdxFilter::new(14, 25.0);
        let signal = make_signal();
        let mut adx_vals = vec![f64::NAN; 15];
        adx_vals[10] = 25.0;
        let iv = make_indicators("adx_14", adx_vals);
        let eval = filter.evaluate(&signal, &[], 10, &iv);
        assert!(eval.verdict.is_passed());
    }

    #[test]
    fn nan_guard() {
        let filter = AdxFilter::new(14, 25.0);
        let signal = make_signal();
        let adx_vals = vec![f64::NAN; 15];
        let iv = make_indicators("adx_14", adx_vals);
        let eval = filter.evaluate(&signal, &[], 10, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn missing_indicator_rejects() {
        let filter = AdxFilter::new(14, 25.0);
        let signal = make_signal();
        let iv = IndicatorValues::new();
        let eval = filter.evaluate(&signal, &[], 10, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn filter_state_snapshot() {
        let filter = AdxFilter::new(14, 25.0);
        let signal = make_signal();
        let mut adx_vals = vec![f64::NAN; 15];
        adx_vals[10] = 30.0;
        let iv = make_indicators("adx_14", adx_vals);
        let eval = filter.evaluate(&signal, &[], 10, &iv);
        assert_eq!(eval.filter_state["adx_value"], 30.0);
        assert_eq!(eval.filter_state["threshold"], 25.0);
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(AdxFilter::default_params().name(), "adx_filter");
    }
}
