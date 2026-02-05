use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Single OHLC bar with timestamp and symbol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bar {
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Bar {
    /// Create a new bar
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: String,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self { timestamp, symbol, open, high, low, close, volume }
    }

    /// Validate bar invariants
    pub fn validate(&self) -> Result<(), BarError> {
        if self.high < self.low {
            return Err(BarError::InvalidRange { high: self.high, low: self.low });
        }
        if self.open < 0.0 || self.high < 0.0 || self.low < 0.0 || self.close < 0.0 {
            return Err(BarError::NegativePrice);
        }
        if self.volume < 0.0 {
            return Err(BarError::NegativeVolume);
        }
        if !(self.low..=self.high).contains(&self.open) {
            return Err(BarError::OpenOutOfRange);
        }
        if !(self.low..=self.high).contains(&self.close) {
            return Err(BarError::CloseOutOfRange);
        }
        Ok(())
    }

    /// Check if bar is bullish (close > open)
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// Get bar range (high - low)
    pub fn range(&self) -> f64 {
        self.high - self.low
    }
}

#[derive(Debug, Error)]
pub enum BarError {
    #[error("Invalid bar range: high={high}, low={low}")]
    InvalidRange { high: f64, low: f64 },

    #[error("Negative price not allowed")]
    NegativePrice,

    #[error("Negative volume not allowed")]
    NegativeVolume,

    #[error("Open price outside high/low range")]
    OpenOutOfRange,

    #[error("Close price outside high/low range")]
    CloseOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_bar_validate_rejects_inverted_range() {
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            99.0, // high < low (invalid)
            101.0,
            100.0,
            1000.0,
        );
        assert!(bar.validate().is_err());
    }

    #[test]
    fn test_bar_validate_accepts_valid_bar() {
        let bar = Bar::new(Utc::now(), "SPY".into(), 100.0, 105.0, 95.0, 102.0, 1000.0);
        assert!(bar.validate().is_ok());
    }

    #[test]
    fn test_bar_rejects_negative_volume() {
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            105.0,
            95.0,
            102.0,
            -100.0, // invalid
        );
        assert!(matches!(bar.validate(), Err(BarError::NegativeVolume)));
    }
}
