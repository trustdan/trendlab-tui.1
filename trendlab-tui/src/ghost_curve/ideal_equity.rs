//! Ideal equity curve calculation
//!
//! Computes the equity curve assuming perfect fills at ideal prices
//! (no slippage, no spread, no adverse selection).

use chrono::{DateTime, Utc};

/// Ideal equity curve (perfect execution)
#[derive(Debug, Clone)]
pub struct IdealEquity {
    /// Equity values over time
    pub values: Vec<f64>,
    /// Timestamps corresponding to equity values
    pub timestamps: Vec<DateTime<Utc>>,
}

impl IdealEquity {
    /// Create a new ideal equity curve
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            timestamps: Vec::new(),
        }
    }

    /// Add an equity point
    pub fn push(&mut self, timestamp: DateTime<Utc>, equity: f64) {
        self.timestamps.push(timestamp);
        self.values.push(equity);
    }

    /// Get final equity value
    pub fn final_equity(&self) -> Option<f64> {
        self.values.last().copied()
    }

    /// Get total return (final / initial - 1)
    pub fn total_return(&self) -> Option<f64> {
        if self.values.len() < 2 {
            return None;
        }
        let initial = self.values.first()?;
        let final_val = self.values.last()?;
        Some((final_val / initial) - 1.0)
    }

    /// Check if curve is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Get number of points
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

impl Default for IdealEquity {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_ideal_equity_creation() {
        let equity = IdealEquity::new();
        assert!(equity.is_empty());
        assert_eq!(equity.len(), 0);
    }

    #[test]
    fn test_ideal_equity_push() {
        let mut equity = IdealEquity::new();
        let ts = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();

        equity.push(ts, 10000.0);
        equity.push(ts, 10500.0);

        assert_eq!(equity.len(), 2);
        assert_eq!(equity.final_equity(), Some(10500.0));
    }

    #[test]
    fn test_ideal_equity_total_return() {
        let mut equity = IdealEquity::new();
        let ts = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();

        equity.push(ts, 10000.0);
        equity.push(ts, 11000.0);

        assert!((equity.total_return().unwrap() - 0.1).abs() < 0.0001); // 10% return
    }

    #[test]
    fn test_ideal_equity_empty_return() {
        let equity = IdealEquity::new();
        assert_eq!(equity.total_return(), None);
    }
}
