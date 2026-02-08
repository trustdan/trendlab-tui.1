//! Cost model — slippage and commission calculation.
//!
//! Slippage is directional: buyers pay more (higher price), sellers receive less (lower price).
//! Commission is symmetric per-side in basis points.
//! Tick rounding is applied after slippage: buy rounds up, sell rounds down.

use crate::components::execution::ExecutionPreset;
use crate::domain::instrument::{round_to_tick, OrderSide};

/// Cost model for execution friction (slippage + commission).
///
/// Current implementation uses fixed basis points. The struct design allows
/// future replacement with ATR-scaled or distribution-sampled models.
#[derive(Debug, Clone)]
pub struct CostModel {
    /// Slippage in basis points, applied directionally.
    pub slippage_bps: f64,
    /// Commission in basis points per side.
    pub commission_bps: f64,
}

impl CostModel {
    pub fn new(slippage_bps: f64, commission_bps: f64) -> Self {
        Self {
            slippage_bps,
            commission_bps,
        }
    }

    pub fn from_preset(preset: ExecutionPreset) -> Self {
        Self {
            slippage_bps: preset.slippage_bps(),
            commission_bps: preset.commission_bps(),
        }
    }

    pub fn frictionless() -> Self {
        Self::new(0.0, 0.0)
    }

    /// Apply slippage to a raw fill price.
    ///
    /// Directional: buyers get a worse (higher) price, sellers get a worse (lower) price.
    /// Returns `(slipped_price, slippage_dollar_amount)`.
    pub fn apply_slippage(&self, raw_price: f64, side: OrderSide, quantity: f64) -> (f64, f64) {
        if self.slippage_bps == 0.0 {
            return (raw_price, 0.0);
        }
        let slip_fraction = self.slippage_bps / 10_000.0;
        match side {
            OrderSide::Buy => {
                let slipped = raw_price * (1.0 + slip_fraction);
                let amount = (slipped - raw_price) * quantity;
                (slipped, amount)
            }
            OrderSide::Sell => {
                let slipped = raw_price * (1.0 - slip_fraction);
                let amount = (raw_price - slipped) * quantity;
                (slipped, amount)
            }
        }
    }

    /// Apply slippage + tick rounding to a raw fill price.
    ///
    /// After slippage, the price is rounded to the nearest tick in the adverse
    /// direction (buy rounds up, sell rounds down).
    /// Returns `(final_price, slippage_dollar_amount)`.
    pub fn apply_slippage_with_tick(
        &self,
        raw_price: f64,
        side: OrderSide,
        quantity: f64,
        tick_size: f64,
    ) -> (f64, f64) {
        let (slipped, _) = self.apply_slippage(raw_price, side, quantity);
        let ticked = round_to_tick(slipped, tick_size, side);
        let slip_amount = match side {
            OrderSide::Buy => (ticked - raw_price) * quantity,
            OrderSide::Sell => (raw_price - ticked) * quantity,
        };
        (ticked, slip_amount.max(0.0))
    }

    /// Compute commission for a fill.
    ///
    /// `commission = fill_price * quantity * (commission_bps / 10_000)`
    pub fn compute_commission(&self, fill_price: f64, quantity: f64) -> f64 {
        fill_price * quantity * (self.commission_bps / 10_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frictionless_returns_raw_price() {
        let cost = CostModel::frictionless();
        let (price, slip) = cost.apply_slippage(100.0, OrderSide::Buy, 50.0);
        assert_eq!(price, 100.0);
        assert_eq!(slip, 0.0);
        assert_eq!(cost.compute_commission(100.0, 50.0), 0.0);
    }

    #[test]
    fn buy_slippage_increases_price() {
        let cost = CostModel::new(10.0, 0.0); // 10 bps
        let (price, slip) = cost.apply_slippage(100.0, OrderSide::Buy, 100.0);
        // 100 * (1 + 10/10000) = 100.10
        assert!((price - 100.10).abs() < 1e-10);
        assert!((slip - 10.0).abs() < 1e-10); // 0.10 * 100 shares
    }

    #[test]
    fn sell_slippage_decreases_price() {
        let cost = CostModel::new(10.0, 0.0); // 10 bps
        let (price, slip) = cost.apply_slippage(100.0, OrderSide::Sell, 100.0);
        // 100 * (1 - 10/10000) = 99.90
        assert!((price - 99.90).abs() < 1e-10);
        assert!((slip - 10.0).abs() < 1e-10);
    }

    #[test]
    fn commission_calculation() {
        let cost = CostModel::new(0.0, 5.0); // 5 bps commission
        let comm = cost.compute_commission(100.0, 1000.0);
        // 100 * 1000 * 5/10000 = 50
        assert!((comm - 50.0).abs() < 1e-10);
    }

    #[test]
    fn realistic_preset() {
        let cost = CostModel::from_preset(ExecutionPreset::Realistic);
        assert_eq!(cost.slippage_bps, 5.0);
        assert_eq!(cost.commission_bps, 5.0);
    }

    #[test]
    fn hostile_preset_highest_costs() {
        let frictionless = CostModel::from_preset(ExecutionPreset::Frictionless);
        let realistic = CostModel::from_preset(ExecutionPreset::Realistic);
        let hostile = CostModel::from_preset(ExecutionPreset::Hostile);

        assert!(hostile.slippage_bps > realistic.slippage_bps);
        assert!(realistic.slippage_bps > frictionless.slippage_bps);
        assert!(hostile.commission_bps > realistic.commission_bps);
    }

    #[test]
    fn slippage_with_tick_rounding_buy() {
        let cost = CostModel::new(10.0, 0.0); // 10 bps
        let (price, _slip) = cost.apply_slippage_with_tick(100.0, OrderSide::Buy, 100.0, 0.01);
        // 100 * 1.001 = 100.10 — already on tick
        assert!((price - 100.10).abs() < 1e-10);

        // Test non-tick-aligned result
        let (price2, _) = cost.apply_slippage_with_tick(100.003, OrderSide::Buy, 100.0, 0.01);
        // 100.003 * 1.001 = 100.103003 → rounds UP to 100.11
        assert!((price2 - 100.11).abs() < 1e-10);
    }

    #[test]
    fn slippage_with_tick_rounding_sell() {
        let cost = CostModel::new(10.0, 0.0); // 10 bps
        let (price, _) = cost.apply_slippage_with_tick(100.003, OrderSide::Sell, 100.0, 0.01);
        // 100.003 * 0.999 = 99.902997 → rounds DOWN to 99.90
        assert!((price - 99.90).abs() < 1e-10);
    }
}
