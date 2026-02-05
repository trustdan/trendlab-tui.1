//! Position Sizers — determine trade quantity
//!
//! Sizers translate dollar amounts or risk budgets into share quantities.
//! They are portfolio-aware (use equity) but signal-agnostic (don't change logic based on signal type).

pub mod fixed;
pub mod atr_risk;

pub use fixed::FixedSizer;
pub use atr_risk::AtrRiskSizer;

use crate::domain::Bar;
use crate::signals::SignalIntent;

/// Position sizing logic
///
/// # Responsibilities
/// - Convert equity + intent + bar data → trade quantity
/// - Apply risk management (e.g., risk % of equity per trade)
/// - Respect minimum/maximum position limits
///
/// # Non-Responsibilities
/// - Sizers do NOT decide entry/exit (that's the signal's job)
/// - Sizers do NOT choose order types (that's the order policy's job)
pub trait Sizer: Send + Sync {
    /// Calculate position size
    ///
    /// # Arguments
    /// - `equity`: Current portfolio equity
    /// - `intent`: Signal intent (Long/Short/Flat)
    /// - `bar`: Current bar for price context
    ///
    /// # Returns
    /// Quantity (number of shares/contracts) to trade.
    /// Returns 0.0 for Flat intent or insufficient equity.
    fn size(&self, equity: f64, intent: SignalIntent, bar: &Bar) -> f64;

    /// Sizer name for manifest/logging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    struct DummySizer;

    impl Sizer for DummySizer {
        fn size(&self, _equity: f64, _intent: SignalIntent, _bar: &Bar) -> f64 {
            100.0
        }

        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn test_sizer_trait_compiles() {
        let sizer = DummySizer;
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            101.0,
            99.0,
            100.5,
            1000000.0,
        );
        let qty = sizer.size(10000.0, SignalIntent::Long, &bar);
        assert_eq!(qty, 100.0);
    }
}
