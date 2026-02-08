//! Indicator precomputation orchestration.
//!
//! All indicators are computed once before the bar loop begins.
//! Results are stored in `IndicatorValues` containers, one per symbol.

use crate::components::indicator::{Indicator, IndicatorValues};
use crate::domain::Bar;
use std::collections::HashMap;

/// Precompute all indicators for all symbols before the bar loop.
///
/// Returns a per-symbol `IndicatorValues` container with all indicator series
/// ready for per-bar lookup during the event loop.
pub fn precompute_indicators(
    bars_by_symbol: &HashMap<String, Vec<Bar>>,
    indicators: &[Box<dyn Indicator>],
) -> HashMap<String, IndicatorValues> {
    let mut result = HashMap::new();

    for (symbol, bars) in bars_by_symbol {
        let mut iv = IndicatorValues::new();
        for indicator in indicators {
            let series = indicator.compute(bars);
            debug_assert_eq!(
                series.len(),
                bars.len(),
                "indicator '{}' produced {} values for {} bars (symbol={})",
                indicator.name(),
                series.len(),
                bars.len(),
                symbol
            );
            iv.insert(indicator.name(), series);
        }
        result.insert(symbol.clone(), iv);
    }

    result
}

/// Compute the warmup length from a set of indicators.
///
/// The warmup is the maximum lookback across all indicators. No signals or
/// orders are generated during the warmup period.
pub fn compute_warmup(indicators: &[Box<dyn Indicator>]) -> usize {
    indicators.iter().map(|i| i.lookback()).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{make_bars, Ema, Sma};

    #[test]
    fn precompute_single_symbol_single_indicator() {
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let mut bars_by_symbol = HashMap::new();
        bars_by_symbol.insert("SPY".to_string(), bars);

        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(Sma::new(3))];
        let result = precompute_indicators(&bars_by_symbol, &indicators);

        assert!(result.contains_key("SPY"));
        let iv = &result["SPY"];
        assert_eq!(iv.len(), 1);
        assert!(iv.get("sma_3", 0).unwrap().is_nan());
        assert!(iv.get("sma_3", 1).unwrap().is_nan());
        // SMA[2] = mean(10,11,12) = 11.0
        let val = iv.get("sma_3", 2).unwrap();
        assert!((val - 11.0).abs() < 1e-10);
    }

    #[test]
    fn precompute_multiple_indicators() {
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let mut bars_by_symbol = HashMap::new();
        bars_by_symbol.insert("SPY".to_string(), bars);

        let indicators: Vec<Box<dyn Indicator>> =
            vec![Box::new(Sma::new(3)), Box::new(Ema::new(3))];
        let result = precompute_indicators(&bars_by_symbol, &indicators);

        let iv = &result["SPY"];
        assert_eq!(iv.len(), 2);
        assert!(iv.get("sma_3", 2).is_some());
        assert!(iv.get("ema_3", 2).is_some());
    }

    #[test]
    fn precompute_multiple_symbols() {
        let spy_bars = make_bars(&[100.0, 101.0, 102.0, 103.0, 104.0]);
        let qqq_bars = make_bars(&[200.0, 201.0, 202.0, 203.0, 204.0]);
        let mut bars_by_symbol = HashMap::new();
        bars_by_symbol.insert("SPY".to_string(), spy_bars);
        bars_by_symbol.insert("QQQ".to_string(), qqq_bars);

        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(Sma::new(3))];
        let result = precompute_indicators(&bars_by_symbol, &indicators);

        assert!(result.contains_key("SPY"));
        assert!(result.contains_key("QQQ"));
        // SPY SMA[2] = mean(100,101,102) = 101
        let spy_sma = result["SPY"].get("sma_3", 2).unwrap();
        assert!((spy_sma - 101.0).abs() < 1e-10);
        // QQQ SMA[2] = mean(200,201,202) = 201
        let qqq_sma = result["QQQ"].get("sma_3", 2).unwrap();
        assert!((qqq_sma - 201.0).abs() < 1e-10);
    }

    #[test]
    fn compute_warmup_max_lookback() {
        let indicators: Vec<Box<dyn Indicator>> = vec![
            Box::new(Sma::new(5)),  // lookback 4
            Box::new(Ema::new(20)), // lookback 19
            Box::new(Sma::new(10)), // lookback 9
        ];
        assert_eq!(compute_warmup(&indicators), 19);
    }

    #[test]
    fn compute_warmup_empty() {
        let indicators: Vec<Box<dyn Indicator>> = vec![];
        assert_eq!(compute_warmup(&indicators), 0);
    }
}
