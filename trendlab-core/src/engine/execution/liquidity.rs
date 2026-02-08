//! Liquidity constraints — participation limits and remainder policies.
//!
//! Optional feature: when enabled, limits the fill quantity to a fraction
//! of the bar's volume. The unfilled remainder is either carried to the next
//! bar or cancelled.

use serde::{Deserialize, Serialize};

/// Policy for handling unfilled remainder when the liquidity limit is hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemainderPolicy {
    /// Carry unfilled remainder to the next bar as a pending order.
    Carry,
    /// Cancel the unfilled remainder immediately.
    Cancel,
}

/// Liquidity constraint configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityPolicy {
    /// Maximum participation rate as a fraction of bar volume (0.0 to 1.0).
    /// Example: 0.10 means fill at most 10% of the bar's volume.
    pub max_participation: f64,
    /// What to do with the unfilled remainder.
    pub remainder: RemainderPolicy,
}

impl LiquidityPolicy {
    pub fn new(max_participation: f64, remainder: RemainderPolicy) -> Self {
        debug_assert!(
            (0.0..=1.0).contains(&max_participation),
            "participation rate must be 0.0 to 1.0"
        );
        Self {
            max_participation,
            remainder,
        }
    }

    /// Maximum fillable quantity given bar volume.
    pub fn max_fill_qty(&self, bar_volume: u64) -> f64 {
        bar_volume as f64 * self.max_participation
    }

    /// Apply liquidity constraint to a desired fill quantity.
    ///
    /// Returns `(fill_qty, remainder_qty)`. If no constraint binds,
    /// `remainder_qty` is zero.
    pub fn constrain(&self, desired_qty: f64, bar_volume: u64) -> (f64, f64) {
        let max_qty = self.max_fill_qty(bar_volume);
        if desired_qty <= max_qty {
            (desired_qty, 0.0)
        } else {
            (max_qty, desired_qty - max_qty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constraint_binds() {
        let policy = LiquidityPolicy::new(0.10, RemainderPolicy::Carry);
        let (fill, remainder) = policy.constrain(100.0, 10_000);
        assert_eq!(fill, 100.0); // 100 <= 1000 (10% of 10k)
        assert_eq!(remainder, 0.0);
    }

    #[test]
    fn constraint_limits_fill() {
        let policy = LiquidityPolicy::new(0.10, RemainderPolicy::Carry);
        let (fill, remainder) = policy.constrain(2000.0, 10_000);
        // Max = 10000 * 0.10 = 1000
        assert_eq!(fill, 1000.0);
        assert_eq!(remainder, 1000.0);
    }

    #[test]
    fn exact_limit_fills_completely() {
        let policy = LiquidityPolicy::new(0.10, RemainderPolicy::Cancel);
        let (fill, remainder) = policy.constrain(1000.0, 10_000);
        assert_eq!(fill, 1000.0);
        assert_eq!(remainder, 0.0);
    }

    #[test]
    fn zero_volume_fills_nothing() {
        let policy = LiquidityPolicy::new(0.10, RemainderPolicy::Carry);
        let (fill, remainder) = policy.constrain(100.0, 0);
        assert_eq!(fill, 0.0);
        assert_eq!(remainder, 100.0);
    }

    #[test]
    fn max_fill_qty() {
        let policy = LiquidityPolicy::new(0.05, RemainderPolicy::Cancel);
        assert_eq!(policy.max_fill_qty(1_000_000), 50_000.0);
    }
}
