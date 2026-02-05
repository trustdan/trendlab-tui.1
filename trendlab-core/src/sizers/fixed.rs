//! Fixed Position Sizer
//!
//! Simplest sizer: trade a fixed quantity or fixed notional amount.

use crate::domain::Bar;
use crate::signals::SignalIntent;
use crate::sizers::Sizer;

/// Fixed position sizer
///
/// Two modes:
/// 1. **Fixed Shares**: Always trade N shares (e.g., 100 shares per trade)
/// 2. **Fixed Notional**: Always trade $X worth (e.g., $10,000 per trade)
#[derive(Debug, Clone)]
pub enum FixedSizer {
    /// Fixed number of shares per trade
    Shares { quantity: f64 },

    /// Fixed dollar amount per trade
    Notional { amount: f64 },
}

impl FixedSizer {
    /// Create fixed shares sizer
    pub fn shares(quantity: f64) -> Self {
        assert!(quantity > 0.0, "quantity must be > 0");
        Self::Shares { quantity }
    }

    /// Create fixed notional sizer
    pub fn notional(amount: f64) -> Self {
        assert!(amount > 0.0, "amount must be > 0");
        Self::Notional { amount }
    }
}

impl Sizer for FixedSizer {
    fn size(&self, equity: f64, intent: SignalIntent, bar: &Bar) -> f64 {
        // No position for flat intent
        if intent == SignalIntent::Flat {
            return 0.0;
        }

        // Insufficient equity check
        if equity <= 0.0 {
            return 0.0;
        }

        match self {
            Self::Shares { quantity } => *quantity,
            Self::Notional { amount } => {
                // Convert notional to shares
                let price = bar.close;
                if price <= 0.0 {
                    return 0.0;
                }
                amount / price
            }
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Shares { .. } => "FixedShares",
            Self::Notional { .. } => "FixedNotional",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar::new(Utc::now(), "SPY".into(), 100.0, 105.0, 95.0, close, 1000000.0)
    }

    #[test]
    fn test_fixed_shares() {
        let sizer = FixedSizer::shares(100.0);
        let bar = make_bar(100.0);

        let qty = sizer.size(10000.0, SignalIntent::Long, &bar);
        assert_eq!(qty, 100.0);
    }

    #[test]
    fn test_fixed_notional() {
        let sizer = FixedSizer::notional(10000.0);
        let bar = make_bar(100.0);

        let qty = sizer.size(50000.0, SignalIntent::Long, &bar);
        assert_eq!(qty, 100.0); // $10000 / $100 = 100 shares
    }

    #[test]
    fn test_fixed_notional_scales_with_price() {
        let sizer = FixedSizer::notional(10000.0);
        let bar_cheap = make_bar(50.0);
        let bar_expensive = make_bar(200.0);

        let qty_cheap = sizer.size(50000.0, SignalIntent::Long, &bar_cheap);
        let qty_expensive = sizer.size(50000.0, SignalIntent::Long, &bar_expensive);

        assert_eq!(qty_cheap, 200.0); // $10000 / $50 = 200 shares
        assert_eq!(qty_expensive, 50.0); // $10000 / $200 = 50 shares
    }

    #[test]
    fn test_flat_intent_returns_zero() {
        let sizer = FixedSizer::shares(100.0);
        let bar = make_bar(100.0);

        let qty = sizer.size(10000.0, SignalIntent::Flat, &bar);
        assert_eq!(qty, 0.0);
    }

    #[test]
    fn test_zero_equity_returns_zero() {
        let sizer = FixedSizer::shares(100.0);
        let bar = make_bar(100.0);

        let qty = sizer.size(0.0, SignalIntent::Long, &bar);
        assert_eq!(qty, 0.0);
    }
}
