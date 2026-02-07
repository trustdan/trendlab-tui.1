//! Signal filter — gates entry signals based on market conditions.
//!
//! Filters evaluate signal events and produce `SignalEvaluation` records.
//! A pass-through "no filter" is the default.

use crate::domain::Bar;

use super::indicator::IndicatorValues;
use super::signal::{SignalEvaluation, SignalEvent};

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
