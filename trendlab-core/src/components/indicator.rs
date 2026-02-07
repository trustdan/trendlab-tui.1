//! Indicator trait and precomputed indicator values container.
//!
//! Indicators are pure functions: bar history in, numeric series out.
//! They are precomputed once before the bar loop and fed per-bar into
//! the event loop. No recomputation on each bar.

use crate::domain::Bar;
use std::collections::HashMap;

/// Trait for indicators.
///
/// Indicators take a full bar series and produce a numeric output series of
/// the same length. The first `lookback()` values should be `f64::NAN` (warmup).
///
/// # Look-ahead contamination guard
/// No indicator value at bar t may depend on price data from bar t+1 or later.
/// Every indicator must pass the truncated-vs-full series test.
pub trait Indicator: Send + Sync {
    /// Human-readable name (e.g., "sma_20", "atr_14").
    fn name(&self) -> &str;

    /// Number of bars needed before the indicator produces valid output.
    fn lookback(&self) -> usize;

    /// Compute the indicator for the entire bar series.
    ///
    /// Returns a `Vec<f64>` of the same length as `bars`.
    /// The first `lookback()` values should be `f64::NAN`.
    fn compute(&self, bars: &[Bar]) -> Vec<f64>;
}

/// Container for precomputed indicator values.
///
/// Built once before the bar loop, then queried by bar index during the loop.
#[derive(Debug, Clone, Default)]
pub struct IndicatorValues {
    series: HashMap<String, Vec<f64>>,
}

impl IndicatorValues {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a named indicator series.
    pub fn insert(&mut self, name: impl Into<String>, values: Vec<f64>) {
        self.series.insert(name.into(), values);
    }

    /// Get the indicator value at a specific bar index.
    pub fn get(&self, name: &str, bar_index: usize) -> Option<f64> {
        self.series
            .get(name)
            .and_then(|v| v.get(bar_index).copied())
    }

    /// Get the full series for a named indicator.
    pub fn get_series(&self, name: &str) -> Option<&[f64]> {
        self.series.get(name).map(|v| v.as_slice())
    }

    /// Number of indicator series stored.
    pub fn len(&self) -> usize {
        self.series.len()
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_values_insert_and_get() {
        let mut iv = IndicatorValues::new();
        iv.insert(
            "sma_20",
            vec![f64::NAN; 19]
                .into_iter()
                .chain(vec![100.0, 101.0])
                .collect(),
        );
        assert!(iv.get("sma_20", 0).unwrap().is_nan());
        assert_eq!(iv.get("sma_20", 19), Some(100.0));
        assert_eq!(iv.get("sma_20", 20), Some(101.0));
        assert_eq!(iv.get("sma_20", 21), None); // out of bounds
    }

    #[test]
    fn indicator_values_missing_name() {
        let iv = IndicatorValues::new();
        assert_eq!(iv.get("nonexistent", 0), None);
    }

    #[test]
    fn indicator_values_len() {
        let mut iv = IndicatorValues::new();
        assert!(iv.is_empty());
        iv.insert("sma", vec![1.0, 2.0]);
        iv.insert("ema", vec![1.0, 2.0]);
        assert_eq!(iv.len(), 2);
    }
}
