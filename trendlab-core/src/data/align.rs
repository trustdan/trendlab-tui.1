//! Multi-symbol time alignment.
//!
//! Given bars for multiple symbols, align them to a common timeline.
//! Missing bars get strict NaN (no forward-fill of tradable price data).

use super::provider::RawBar;
use chrono::NaiveDate;
use std::collections::{BTreeSet, HashMap};

/// Aligned bar data for multiple symbols on a common timeline.
#[derive(Debug)]
pub struct AlignedData {
    /// The common date axis (sorted ascending).
    pub dates: Vec<NaiveDate>,
    /// Bars per symbol, aligned to the common timeline.
    /// Each inner Vec has the same length as `dates`.
    pub bars: HashMap<String, Vec<RawBar>>,
    /// Symbols included.
    pub symbols: Vec<String>,
}

/// Align multiple symbols to a common timeline.
///
/// For each date in the union of all symbols' dates, each symbol either
/// has a real bar or gets a void bar (all OHLCV set to NaN).
pub fn align_symbols(symbol_bars: HashMap<String, Vec<RawBar>>) -> AlignedData {
    // Collect the union of all dates
    let mut all_dates = BTreeSet::new();
    for bars in symbol_bars.values() {
        for bar in bars {
            all_dates.insert(bar.date);
        }
    }
    let dates: Vec<NaiveDate> = all_dates.into_iter().collect();

    // Build a lookup per symbol: date → bar
    let symbols: Vec<String> = symbol_bars.keys().cloned().collect();
    let mut aligned: HashMap<String, Vec<RawBar>> = HashMap::new();

    for (symbol, bars) in &symbol_bars {
        let mut date_map: HashMap<NaiveDate, &RawBar> = HashMap::new();
        for bar in bars {
            date_map.insert(bar.date, bar);
        }

        let aligned_bars: Vec<RawBar> = dates
            .iter()
            .map(|date| {
                date_map
                    .get(date)
                    .map(|b| (*b).clone())
                    .unwrap_or_else(|| void_bar(symbol, *date))
            })
            .collect();

        aligned.insert(symbol.clone(), aligned_bars);
    }

    AlignedData {
        dates,
        bars: aligned,
        symbols,
    }
}

/// Create a void bar (all OHLCV = NaN) for a missing date.
fn void_bar(_symbol: &str, date: NaiveDate) -> RawBar {
    RawBar {
        date,
        open: f64::NAN,
        high: f64::NAN,
        low: f64::NAN,
        close: f64::NAN,
        volume: 0,
        adj_close: f64::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(_symbol: &str, date: &str, close: f64) -> RawBar {
        RawBar {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            open: close - 1.0,
            high: close + 1.0,
            low: close - 2.0,
            close,
            volume: 1000,
            adj_close: close,
        }
    }

    #[test]
    fn align_fills_missing_with_nan() {
        let mut input = HashMap::new();
        input.insert(
            "SPY".into(),
            vec![
                bar("SPY", "2024-01-02", 100.0),
                bar("SPY", "2024-01-03", 101.0),
                bar("SPY", "2024-01-04", 102.0),
            ],
        );
        input.insert(
            "QQQ".into(),
            vec![
                bar("QQQ", "2024-01-02", 200.0),
                // QQQ missing 2024-01-03
                bar("QQQ", "2024-01-04", 202.0),
            ],
        );

        let aligned = align_symbols(input);

        assert_eq!(aligned.dates.len(), 3);
        assert_eq!(aligned.bars["SPY"].len(), 3);
        assert_eq!(aligned.bars["QQQ"].len(), 3);

        // SPY has all bars
        assert_eq!(aligned.bars["SPY"][1].close, 101.0);

        // QQQ has a void bar on 2024-01-03
        assert!(aligned.bars["QQQ"][1].close.is_nan());
    }

    #[test]
    fn single_symbol_no_alignment_needed() {
        let mut input = HashMap::new();
        input.insert("SPY".into(), vec![bar("SPY", "2024-01-02", 100.0)]);

        let aligned = align_symbols(input);
        assert_eq!(aligned.dates.len(), 1);
        assert_eq!(aligned.bars["SPY"].len(), 1);
        assert_eq!(aligned.bars["SPY"][0].close, 100.0);
    }
}
