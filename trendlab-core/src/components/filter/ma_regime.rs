//! MA regime signal filter - gates signals by trend regime.
//!
//! Passes signals when price is above (or below) a moving average,
//! indicating the desired market regime.

use crate::components::indicator::IndicatorValues;
use crate::components::signal::{FilterVerdict, SignalEvaluation, SignalEvent};
use crate::domain::Bar;
use std::collections::HashMap;

use super::SignalFilter;

/// Regime direction for MA filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegimeDirection {
    /// Price must be above the MA to pass.
    Above,
    /// Price must be below the MA to pass.
    Below,
}

/// MA regime filter.
///
/// Passes signals when price is in the desired regime relative to a simple
/// moving average. Use `Above` for trend-following longs, `Below` for shorts.
#[derive(Debug, Clone)]
pub struct MaRegimeFilter {
    pub period: usize,
    pub regime: RegimeDirection,
    indicator_key: String,
}

impl MaRegimeFilter {
    pub fn new(period: usize, regime: RegimeDirection) -> Self {
        assert!(period >= 1, "period must be >= 1");
        Self {
            period,
            regime,
            indicator_key: format!("sma_{period}"),
        }
    }

    pub fn default_params() -> Self {
        Self::new(200, RegimeDirection::Above)
    }
}

impl SignalFilter for MaRegimeFilter {
    fn name(&self) -> &str {
        "ma_regime"
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

        let sma_value = indicators.get(&self.indicator_key, bar_index);

        let (verdict, filter_state) = match sma_value {
            Some(sma) if !sma.is_nan() && !close.is_nan() => {
                let mut state = HashMap::new();
                state.insert("sma_value".into(), sma);
                state.insert("close".into(), close);
                let in_regime = match self.regime {
                    RegimeDirection::Above => close >= sma,
                    RegimeDirection::Below => close <= sma,
                };
                if in_regime {
                    (FilterVerdict::Passed, state)
                } else {
                    (FilterVerdict::FilteredByRegime, state)
                }
            }
            _ => (FilterVerdict::FilteredByRegime, HashMap::new()),
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
    fn passes_above_regime_when_close_above_sma() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Above);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let mut sma_vals = vec![f64::NAN; 10];
        sma_vals[5] = 95.0; // close=100 > sma=95
        let iv = make_indicators("sma_20", sma_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(eval.verdict.is_passed());
    }

    #[test]
    fn rejects_above_regime_when_close_below_sma() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Above);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let mut sma_vals = vec![f64::NAN; 10];
        sma_vals[5] = 105.0; // close=100 < sma=105
        let iv = make_indicators("sma_20", sma_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
        assert_eq!(eval.verdict, FilterVerdict::FilteredByRegime);
    }

    #[test]
    fn passes_below_regime_when_close_below_sma() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Below);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let mut sma_vals = vec![f64::NAN; 10];
        sma_vals[5] = 105.0; // close=100 < sma=105
        let iv = make_indicators("sma_20", sma_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(eval.verdict.is_passed());
    }

    #[test]
    fn rejects_below_regime_when_close_above_sma() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Below);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let mut sma_vals = vec![f64::NAN; 10];
        sma_vals[5] = 95.0;
        let iv = make_indicators("sma_20", sma_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn nan_guard() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Above);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let sma_vals = vec![f64::NAN; 10];
        let iv = make_indicators("sma_20", sma_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn missing_indicator_rejects() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Above);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let iv = IndicatorValues::new();
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn filter_state_snapshot() {
        let filter = MaRegimeFilter::new(20, RegimeDirection::Above);
        let signal = make_signal();
        let bars = make_bars_with_close(&[100.0; 10]);
        let mut sma_vals = vec![f64::NAN; 10];
        sma_vals[5] = 95.0;
        let iv = make_indicators("sma_20", sma_vals);
        let eval = filter.evaluate(&signal, &bars, 5, &iv);
        assert_eq!(eval.filter_state["sma_value"], 95.0);
        assert_eq!(eval.filter_state["close"], 100.0);
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(MaRegimeFilter::default_params().name(), "ma_regime");
    }
}
