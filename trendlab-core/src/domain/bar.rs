//! Bar — the fundamental market data unit.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// OHLCV bar for a single symbol on a single day.
///
/// All OHLC columns are split-adjusted (Phase 4 applies adjustment ratios).
/// The `adj_close` column is retained for reference but all calculations use adjusted OHLC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bar {
    pub symbol: String,
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
    pub adj_close: f64,
}

impl Bar {
    /// Returns true if any OHLCV field is NaN (void bar).
    pub fn is_void(&self) -> bool {
        self.open.is_nan()
            || self.high.is_nan()
            || self.low.is_nan()
            || self.close.is_nan()
            || self.adj_close.is_nan()
    }

    /// Basic OHLCV sanity check: high >= low, high >= open, high >= close, etc.
    pub fn is_sane(&self) -> bool {
        if self.is_void() {
            return false;
        }
        self.high >= self.low
            && self.high >= self.open
            && self.high >= self.close
            && self.low <= self.open
            && self.low <= self.close
            && self.open > 0.0
            && self.close > 0.0
    }
}

/// Whether the market is open or closed for a symbol on a given bar.
///
/// A bar with all-NaN OHLCV fields (produced by multi-symbol alignment) has
/// `MarketStatus::Closed`. The void bar policy (architecture invariant #7)
/// defines behavior for closed bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    Open,
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bar() -> Bar {
        Bar {
            symbol: "SPY".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 98.0,
            close: 103.0,
            volume: 50_000,
            adj_close: 103.0,
        }
    }

    #[test]
    fn bar_is_sane() {
        assert!(sample_bar().is_sane());
    }

    #[test]
    fn bar_detects_void() {
        let mut bar = sample_bar();
        bar.open = f64::NAN;
        assert!(bar.is_void());
        assert!(!bar.is_sane());
    }

    #[test]
    fn bar_detects_insane_high_low() {
        let mut bar = sample_bar();
        bar.high = 97.0; // below low
        assert!(!bar.is_sane());
    }

    #[test]
    fn bar_serialization_roundtrip() {
        let bar = sample_bar();
        let json = serde_json::to_string(&bar).unwrap();
        let deser: Bar = serde_json::from_str(&json).unwrap();
        assert_eq!(bar.symbol, deser.symbol);
        assert_eq!(bar.date, deser.date);
        assert_eq!(bar.close, deser.close);
    }
}
