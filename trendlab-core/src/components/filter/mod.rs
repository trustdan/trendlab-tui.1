//! Signal filter — gates entry signals based on market conditions.
//!
//! Filters evaluate signal events and produce `SignalEvaluation` records.
//! A pass-through "no filter" is the default.

pub mod adx_filter;
pub mod ma_regime;
pub mod volatility;

use crate::domain::Bar;
use std::collections::HashMap;

use super::indicator::IndicatorValues;
use super::signal::{FilterVerdict, SignalEvaluation, SignalEvent};

/// Trait for signal filters.
///
/// Filters gate entry signals based on market conditions (trend regime,
/// volatility level, momentum strength). They produce a `SignalEvaluation`
/// record that captures the verdict and the filter's state at evaluation time.
///
/// # Architecture invariant
/// Filters must not reference portfolio state — they evaluate market conditions only.
pub trait SignalFilter: Send + Sync {
    /// Human-readable name (e.g., "adx_filter", "no_filter").
    fn name(&self) -> &str;

    /// Evaluate whether a signal should be allowed through.
    ///
    /// Returns a `SignalEvaluation` with the verdict and filter state snapshot.
    fn evaluate(
        &self,
        signal: &SignalEvent,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> SignalEvaluation;
}

/// No-op filter — always passes signals through.
pub struct NoFilter;

impl SignalFilter for NoFilter {
    fn name(&self) -> &str {
        "no_filter"
    }

    fn evaluate(
        &self,
        signal: &SignalEvent,
        _bars: &[Bar],
        _bar_index: usize,
        _indicators: &IndicatorValues,
    ) -> SignalEvaluation {
        SignalEvaluation {
            signal_event_id: signal.id,
            filter_name: "no_filter".to_string(),
            verdict: FilterVerdict::Passed,
            filter_state: HashMap::new(),
        }
    }
}

// Re-export concrete filter types.
pub use adx_filter::AdxFilter;
pub use ma_regime::{MaRegimeFilter, RegimeDirection};
pub use volatility::VolatilityFilter;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SignalEventId;
    use chrono::NaiveDate;

    fn make_signal() -> SignalEvent {
        SignalEvent {
            id: SignalEventId(1),
            bar_index: 10,
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "SPY".into(),
            direction: super::super::signal::SignalDirection::Long,
            strength: 0.8,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn no_filter_always_passes() {
        let filter = NoFilter;
        let signal = make_signal();
        let iv = IndicatorValues::new();
        let eval = filter.evaluate(&signal, &[], 0, &iv);
        assert!(eval.verdict.is_passed());
        assert_eq!(eval.filter_name, "no_filter");
        assert_eq!(eval.signal_event_id, signal.id);
    }

    #[test]
    fn no_filter_name() {
        assert_eq!(NoFilter.name(), "no_filter");
    }
}
