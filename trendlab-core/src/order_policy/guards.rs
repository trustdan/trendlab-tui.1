//! Rejection guards — evaluate bars and portfolio state to reject intents
//!
//! Guards implement the 4 canonical rejection reasons:
//! - VolatilityGuard: reject when intrabar range is excessive
//! - LiquidityGuard: reject when volume is too low
//! - MarginGuard: reject when cash is insufficient
//! - RiskGuard: reject when too many positions are open

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::Bar;

/// Why an intent was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectionReason {
    VolatilityGuard,
    LiquidityGuard,
    MarginGuard,
    RiskGuard,
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectionReason::VolatilityGuard => write!(f, "VolatilityGuard"),
            RejectionReason::LiquidityGuard => write!(f, "LiquidityGuard"),
            RejectionReason::MarginGuard => write!(f, "MarginGuard"),
            RejectionReason::RiskGuard => write!(f, "RiskGuard"),
        }
    }
}

/// A rejected intent record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedIntent {
    pub bar_index: usize,
    pub timestamp: DateTime<Utc>,
    pub signal: String,
    pub reason: RejectionReason,
    pub context: String,
}

/// Trait for guards that can reject intents based on bar/portfolio state.
pub trait Guard: Send + Sync {
    /// Evaluate whether to reject a signal on this bar.
    ///
    /// Returns `Some(RejectedIntent)` if the bar should be rejected, `None` otherwise.
    fn evaluate(
        &self,
        bar: &Bar,
        bar_index: usize,
        cash: f64,
        open_positions: usize,
    ) -> Option<RejectedIntent>;

    /// Guard name for logging.
    fn name(&self) -> &str;
}

/// Reject when intrabar range exceeds threshold relative to close.
#[derive(Debug)]
pub struct VolatilityGuard {
    /// Maximum `(high - low) / close` before rejection.
    pub threshold: f64,
}

impl VolatilityGuard {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl Guard for VolatilityGuard {
    fn evaluate(
        &self,
        bar: &Bar,
        bar_index: usize,
        _cash: f64,
        _open_positions: usize,
    ) -> Option<RejectedIntent> {
        if bar.close == 0.0 {
            return None;
        }
        let range_pct = (bar.high - bar.low) / bar.close;
        if range_pct > self.threshold {
            Some(RejectedIntent {
                bar_index,
                timestamp: bar.timestamp,
                signal: "Long".to_string(),
                reason: RejectionReason::VolatilityGuard,
                context: format!(
                    "range={:.4}, threshold={:.4}",
                    range_pct, self.threshold
                ),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "VolatilityGuard"
    }
}

/// Reject when bar volume is below minimum.
#[derive(Debug)]
pub struct LiquidityGuard {
    pub min_volume: f64,
}

impl LiquidityGuard {
    pub fn new(min_volume: f64) -> Self {
        Self { min_volume }
    }
}

impl Guard for LiquidityGuard {
    fn evaluate(
        &self,
        bar: &Bar,
        bar_index: usize,
        _cash: f64,
        _open_positions: usize,
    ) -> Option<RejectedIntent> {
        if bar.volume < self.min_volume {
            Some(RejectedIntent {
                bar_index,
                timestamp: bar.timestamp,
                signal: "Long".to_string(),
                reason: RejectionReason::LiquidityGuard,
                context: format!(
                    "volume={:.0}, min_volume={:.0}",
                    bar.volume, self.min_volume
                ),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "LiquidityGuard"
    }
}

/// Reject when available cash falls below minimum.
#[derive(Debug)]
pub struct MarginGuard {
    pub min_cash: f64,
}

impl MarginGuard {
    pub fn new(min_cash: f64) -> Self {
        Self { min_cash }
    }
}

impl Guard for MarginGuard {
    fn evaluate(
        &self,
        bar: &Bar,
        bar_index: usize,
        cash: f64,
        _open_positions: usize,
    ) -> Option<RejectedIntent> {
        if cash < self.min_cash {
            Some(RejectedIntent {
                bar_index,
                timestamp: bar.timestamp,
                signal: "Long".to_string(),
                reason: RejectionReason::MarginGuard,
                context: format!("cash={:.2}, min_cash={:.2}", cash, self.min_cash),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "MarginGuard"
    }
}

/// Reject when too many positions are open.
#[derive(Debug)]
pub struct RiskGuard {
    pub max_positions: usize,
}

impl RiskGuard {
    pub fn new(max_positions: usize) -> Self {
        Self { max_positions }
    }
}

impl Guard for RiskGuard {
    fn evaluate(
        &self,
        bar: &Bar,
        bar_index: usize,
        _cash: f64,
        open_positions: usize,
    ) -> Option<RejectedIntent> {
        if open_positions >= self.max_positions {
            Some(RejectedIntent {
                bar_index,
                timestamp: bar.timestamp,
                signal: "Long".to_string(),
                reason: RejectionReason::RiskGuard,
                context: format!(
                    "open_positions={}, max_positions={}",
                    open_positions, self.max_positions
                ),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "RiskGuard"
    }
}

/// Create a default set of guards with reasonable thresholds.
pub fn default_guards() -> Vec<Box<dyn Guard>> {
    vec![
        Box::new(VolatilityGuard::new(0.05)),
        Box::new(LiquidityGuard::new(100_000.0)),
        Box::new(MarginGuard::new(1_000.0)),
        Box::new(RiskGuard::new(10)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_bar(high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar::new(Utc::now(), "SPY".into(), close, high, low, close, volume)
    }

    #[test]
    fn test_volatility_guard_triggers() {
        let guard = VolatilityGuard::new(0.05);
        // range = (110 - 95) / 100 = 0.15 > 0.05
        let bar = test_bar(110.0, 95.0, 100.0, 1_000_000.0);
        let result = guard.evaluate(&bar, 0, 100_000.0, 0);
        assert!(result.is_some());
        assert_eq!(result.unwrap().reason, RejectionReason::VolatilityGuard);
    }

    #[test]
    fn test_volatility_guard_passes() {
        let guard = VolatilityGuard::new(0.05);
        // range = (101 - 99) / 100 = 0.02 < 0.05
        let bar = test_bar(101.0, 99.0, 100.0, 1_000_000.0);
        let result = guard.evaluate(&bar, 0, 100_000.0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_liquidity_guard_triggers() {
        let guard = LiquidityGuard::new(100_000.0);
        let bar = test_bar(101.0, 99.0, 100.0, 50_000.0);
        let result = guard.evaluate(&bar, 0, 100_000.0, 0);
        assert!(result.is_some());
        assert_eq!(result.unwrap().reason, RejectionReason::LiquidityGuard);
    }

    #[test]
    fn test_liquidity_guard_passes() {
        let guard = LiquidityGuard::new(100_000.0);
        let bar = test_bar(101.0, 99.0, 100.0, 200_000.0);
        let result = guard.evaluate(&bar, 0, 100_000.0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_margin_guard_triggers() {
        let guard = MarginGuard::new(1_000.0);
        let bar = test_bar(101.0, 99.0, 100.0, 1_000_000.0);
        let result = guard.evaluate(&bar, 0, 500.0, 0);
        assert!(result.is_some());
        assert_eq!(result.unwrap().reason, RejectionReason::MarginGuard);
    }

    #[test]
    fn test_margin_guard_passes() {
        let guard = MarginGuard::new(1_000.0);
        let bar = test_bar(101.0, 99.0, 100.0, 1_000_000.0);
        let result = guard.evaluate(&bar, 0, 50_000.0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_risk_guard_triggers() {
        let guard = RiskGuard::new(10);
        let bar = test_bar(101.0, 99.0, 100.0, 1_000_000.0);
        let result = guard.evaluate(&bar, 0, 100_000.0, 10);
        assert!(result.is_some());
        assert_eq!(result.unwrap().reason, RejectionReason::RiskGuard);
    }

    #[test]
    fn test_risk_guard_passes() {
        let guard = RiskGuard::new(10);
        let bar = test_bar(101.0, 99.0, 100.0, 1_000_000.0);
        let result = guard.evaluate(&bar, 0, 100_000.0, 5);
        assert!(result.is_none());
    }

    #[test]
    fn test_default_guards() {
        let guards = default_guards();
        assert_eq!(guards.len(), 4);
    }

    #[test]
    fn test_rejection_reason_display() {
        assert_eq!(RejectionReason::VolatilityGuard.to_string(), "VolatilityGuard");
        assert_eq!(RejectionReason::LiquidityGuard.to_string(), "LiquidityGuard");
        assert_eq!(RejectionReason::MarginGuard.to_string(), "MarginGuard");
        assert_eq!(RejectionReason::RiskGuard.to_string(), "RiskGuard");
    }
}
