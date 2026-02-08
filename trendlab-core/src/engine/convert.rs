//! Conversion from data-pipeline types to domain types.
//!
//! The data pipeline uses `RawBar` (no symbol field); the engine uses `domain::Bar`
//! (with symbol). This module bridges the gap with a one-time conversion before the
//! bar loop begins.

use crate::data::align::AlignedData;
use crate::data::provider::RawBar;
use crate::domain::Bar;
use std::collections::HashMap;

/// Convert a single `RawBar` + symbol name into a domain `Bar`.
pub fn raw_to_bar(raw: &RawBar, symbol: &str) -> Bar {
    Bar {
        symbol: symbol.to_string(),
        date: raw.date,
        open: raw.open,
        high: raw.high,
        low: raw.low,
        close: raw.close,
        volume: raw.volume,
        adj_close: raw.adj_close,
    }
}

/// Convert `AlignedData` into per-symbol `Vec<Bar>`.
///
/// This is called once before the bar loop. Each symbol's bars are in date order
/// (matching `AlignedData.dates`).
pub fn aligned_to_bars(aligned: &AlignedData) -> HashMap<String, Vec<Bar>> {
    aligned
        .bars
        .iter()
        .map(|(symbol, raw_bars)| {
            let bars: Vec<Bar> = raw_bars.iter().map(|raw| raw_to_bar(raw, symbol)).collect();
            (symbol.clone(), bars)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn raw_to_bar_copies_fields() {
        let raw = RawBar {
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 99.0,
            close: 103.0,
            volume: 1000,
            adj_close: 103.0,
        };
        let bar = raw_to_bar(&raw, "SPY");
        assert_eq!(bar.symbol, "SPY");
        assert_eq!(bar.date, raw.date);
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.high, 105.0);
        assert_eq!(bar.low, 99.0);
        assert_eq!(bar.close, 103.0);
        assert_eq!(bar.volume, 1000);
        assert_eq!(bar.adj_close, 103.0);
    }

    #[test]
    fn raw_to_bar_preserves_nan_for_void_bars() {
        let raw = RawBar {
            date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
            open: f64::NAN,
            high: f64::NAN,
            low: f64::NAN,
            close: f64::NAN,
            volume: 0,
            adj_close: f64::NAN,
        };
        let bar = raw_to_bar(&raw, "QQQ");
        assert!(bar.is_void());
    }

    #[test]
    fn aligned_to_bars_converts_all_symbols() {
        let mut raw_bars = HashMap::new();
        raw_bars.insert(
            "SPY".to_string(),
            vec![RawBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 103.0,
                volume: 1000,
                adj_close: 103.0,
            }],
        );
        raw_bars.insert(
            "QQQ".to_string(),
            vec![RawBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                open: 200.0,
                high: 210.0,
                low: 198.0,
                close: 205.0,
                volume: 2000,
                adj_close: 205.0,
            }],
        );

        let aligned = AlignedData {
            dates: vec![NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()],
            bars: raw_bars,
            symbols: vec!["SPY".to_string(), "QQQ".to_string()],
        };

        let bars = aligned_to_bars(&aligned);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars["SPY"][0].symbol, "SPY");
        assert_eq!(bars["SPY"][0].close, 103.0);
        assert_eq!(bars["QQQ"][0].symbol, "QQQ");
        assert_eq!(bars["QQQ"][0].close, 205.0);
    }
}
