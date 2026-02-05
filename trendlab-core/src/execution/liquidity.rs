//! Liquidity constraints: model participation limits and capacity realism
//!
//! When order size is large relative to bar volume, we need to:
//! 1. Limit fills to a % of bar volume (participation rate)
//! 2. Handle unfilled remainder (carry, cancel, or partial fill)
//! 3. Use Time-Priority (FIFO) allocation when multiple orders compete

use crate::domain::Bar;

/// Policy for handling unfilled order remainder
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemainderPolicy {
    /// Carry unfilled qty to next bar (order stays Active)
    Carry,
    /// Cancel entire order if cannot fill completely
    Cancel,
    /// Accept partial fill (order becomes PartiallyFilled/Filled)
    PartialFill,
}

/// Liquidity constraint: limits order fills based on bar volume
#[derive(Debug, Clone, Copy)]
pub struct LiquidityConstraint {
    /// Maximum participation rate (e.g., 0.05 = 5% of bar volume)
    pub max_participation: f64,
    /// Policy for unfilled remainder
    pub remainder_policy: RemainderPolicy,
}

impl LiquidityConstraint {
    pub fn new(max_participation: f64, remainder_policy: RemainderPolicy) -> Self {
        assert!(
            max_participation > 0.0 && max_participation <= 1.0,
            "max_participation must be in (0, 1]"
        );

        Self {
            max_participation,
            remainder_policy,
        }
    }

    /// Limit fill quantity based on bar volume
    ///
    /// # Arguments
    /// - `requested_qty`: Desired fill quantity
    /// - `bar_volume`: Volume of current bar
    ///
    /// # Returns
    /// Actual fill quantity (may be less than requested)
    pub fn limit_fill_qty(&self, requested_qty: u32, bar_volume: f64) -> u32 {
        let max_qty = (bar_volume * self.max_participation) as u32;
        requested_qty.min(max_qty)
    }

    /// Check if order can be filled completely
    pub fn can_fill_completely(&self, requested_qty: u32, bar_volume: f64) -> bool {
        let max_qty = (bar_volume * self.max_participation) as u32;
        requested_qty <= max_qty
    }

    /// Allocate volume among competing orders (Time-Priority FIFO)
    ///
    /// # Arguments
    /// - `order_qtys`: Vector of (order_id, requested_qty) sorted by submission time
    /// - `bar`: Current bar data
    ///
    /// # Returns
    /// Vector of (order_id, fill_qty) allocations
    pub fn allocate_fifo<T: Copy>(
        &self,
        order_qtys: Vec<(T, u32)>,
        bar: &Bar,
    ) -> Vec<(T, u32)> {
        let max_volume = (bar.volume * self.max_participation) as u32;
        let mut remaining_volume = max_volume;
        let mut allocations = Vec::new();

        for (order_id, requested_qty) in order_qtys {
            if remaining_volume == 0 {
                // Pool exhausted, no fill
                allocations.push((order_id, 0));
                continue;
            }

            let fill_qty = requested_qty.min(remaining_volume);
            allocations.push((order_id, fill_qty));
            remaining_volume = remaining_volume.saturating_sub(fill_qty);
        }

        allocations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bar(volume: f64) -> Bar {
        Bar {
            timestamp: chrono::Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 102.0,
            low: 98.0,
            close: 101.0,
            volume,
        }
    }

    #[test]
    fn test_limit_fill_qty_no_constraint() {
        let constraint = LiquidityConstraint::new(0.1, RemainderPolicy::Carry);
        let bar = test_bar(1_000_000.0);

        // Request 50,000 shares, 10% of 1M = 100,000 max
        let fill_qty = constraint.limit_fill_qty(50_000, bar.volume);
        assert_eq!(fill_qty, 50_000); // Full fill
    }

    #[test]
    fn test_limit_fill_qty_constrained() {
        let constraint = LiquidityConstraint::new(0.05, RemainderPolicy::Carry);
        let bar = test_bar(1_000_000.0);

        // Request 100,000 shares, 5% of 1M = 50,000 max
        let fill_qty = constraint.limit_fill_qty(100_000, bar.volume);
        assert_eq!(fill_qty, 50_000); // Partial fill
    }

    #[test]
    fn test_can_fill_completely_true() {
        let constraint = LiquidityConstraint::new(0.1, RemainderPolicy::Carry);
        let bar = test_bar(1_000_000.0);

        assert!(constraint.can_fill_completely(80_000, bar.volume));
    }

    #[test]
    fn test_can_fill_completely_false() {
        let constraint = LiquidityConstraint::new(0.05, RemainderPolicy::Carry);
        let bar = test_bar(1_000_000.0);

        assert!(!constraint.can_fill_completely(100_000, bar.volume));
    }

    #[test]
    fn test_fifo_allocation_sufficient_volume() {
        let constraint = LiquidityConstraint::new(0.2, RemainderPolicy::Carry);
        let bar = test_bar(10_000.0);

        // 20% of 10,000 = 2,000 shares available
        let orders = vec![
            (1u32, 800),  // Order 1: 800 shares
            (2u32, 500),  // Order 2: 500 shares
            (3u32, 400),  // Order 3: 400 shares
        ];

        let allocations = constraint.allocate_fifo(orders, &bar);

        assert_eq!(allocations[0], (1, 800)); // Full fill
        assert_eq!(allocations[1], (2, 500)); // Full fill
        assert_eq!(allocations[2], (3, 400)); // Full fill
        // Total: 1,700 < 2,000 (unused capacity: 300)
    }

    #[test]
    fn test_fifo_allocation_exhausted_volume() {
        let constraint = LiquidityConstraint::new(0.1, RemainderPolicy::Carry);
        let bar = test_bar(10_000.0);

        // 10% of 10,000 = 1,000 shares available
        let orders = vec![
            (1u32, 800),  // Order 1: 800 shares
            (2u32, 500),  // Order 2: 500 shares (partial)
            (3u32, 400),  // Order 3: 400 shares (no fill)
        ];

        let allocations = constraint.allocate_fifo(orders, &bar);

        assert_eq!(allocations[0], (1, 800)); // Full fill (remaining: 200)
        assert_eq!(allocations[1], (2, 200)); // Partial fill (remaining: 0)
        assert_eq!(allocations[2], (3, 0));   // No fill (pool exhausted)
    }

    #[test]
    fn test_fifo_allocation_first_order_exhausts() {
        let constraint = LiquidityConstraint::new(0.05, RemainderPolicy::Carry);
        let bar = test_bar(10_000.0);

        // 5% of 10,000 = 500 shares available
        let orders = vec![
            (1u32, 800),  // Order 1: 800 shares (partial)
            (2u32, 500),  // Order 2: 500 shares (no fill)
        ];

        let allocations = constraint.allocate_fifo(orders, &bar);

        assert_eq!(allocations[0], (1, 500)); // Partial fill (pool exhausted)
        assert_eq!(allocations[1], (2, 0));   // No fill
    }

    #[test]
    #[should_panic(expected = "max_participation must be in (0, 1]")]
    fn test_invalid_participation_zero() {
        LiquidityConstraint::new(0.0, RemainderPolicy::Carry);
    }

    #[test]
    #[should_panic(expected = "max_participation must be in (0, 1]")]
    fn test_invalid_participation_over_one() {
        LiquidityConstraint::new(1.5, RemainderPolicy::Carry);
    }

    #[test]
    fn test_remainder_policies() {
        let carry = LiquidityConstraint::new(0.1, RemainderPolicy::Carry);
        let cancel = LiquidityConstraint::new(0.1, RemainderPolicy::Cancel);
        let partial = LiquidityConstraint::new(0.1, RemainderPolicy::PartialFill);

        assert_eq!(carry.remainder_policy, RemainderPolicy::Carry);
        assert_eq!(cancel.remainder_policy, RemainderPolicy::Cancel);
        assert_eq!(partial.remainder_policy, RemainderPolicy::PartialFill);
    }
}
