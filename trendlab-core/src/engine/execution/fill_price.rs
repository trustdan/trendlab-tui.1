//! Fill price computation — applies cost model to raw trigger prices.
//!
//! Takes a raw fill price from the trigger module, applies slippage,
//! tick rounding, and commission to produce the final fill parameters.

use crate::domain::instrument::{Instrument, OrderSide};

use super::cost_model::CostModel;

/// The fully computed fill price with all costs.
#[derive(Debug, Clone)]
pub struct ComputedFill {
    /// Final fill price after slippage + tick rounding.
    pub price: f64,
    /// Dollar amount of slippage applied.
    pub slippage: f64,
    /// Dollar amount of commission.
    pub commission: f64,
}

/// Compute the final fill price from a raw trigger price.
///
/// Applies: slippage (directional) → tick rounding → commission calculation.
pub fn compute_fill(
    raw_price: f64,
    side: OrderSide,
    quantity: f64,
    instrument: &Instrument,
    cost_model: &CostModel,
) -> ComputedFill {
    let (price, slippage) =
        cost_model.apply_slippage_with_tick(raw_price, side, quantity, instrument.tick_size);
    let commission = cost_model.compute_commission(price, quantity);

    ComputedFill {
        price,
        slippage,
        commission,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frictionless_fill() {
        let cost = CostModel::frictionless();
        let inst = Instrument::us_equity("SPY");
        let fill = compute_fill(100.0, OrderSide::Buy, 100.0, &inst, &cost);
        assert_eq!(fill.price, 100.0);
        assert_eq!(fill.slippage, 0.0);
        assert_eq!(fill.commission, 0.0);
    }

    #[test]
    fn realistic_buy_fill() {
        let cost = CostModel::new(5.0, 5.0); // 5 bps each
        let inst = Instrument::us_equity("SPY");
        let fill = compute_fill(100.0, OrderSide::Buy, 100.0, &inst, &cost);
        // slipped: 100 * 1.0005 = 100.05 → tick rounded up to 100.05
        assert!((fill.price - 100.05).abs() < 1e-10);
        assert!(fill.slippage > 0.0);
        // commission: 100.05 * 100 * 5/10000 = 5.0025
        assert!((fill.commission - 5.0025).abs() < 1e-4);
    }

    #[test]
    fn realistic_sell_fill() {
        let cost = CostModel::new(5.0, 5.0);
        let inst = Instrument::us_equity("SPY");
        let fill = compute_fill(100.0, OrderSide::Sell, 100.0, &inst, &cost);
        // slipped: 100 * 0.9995 = 99.95 → tick rounded down to 99.95
        assert!((fill.price - 99.95).abs() < 1e-10);
        assert!(fill.slippage > 0.0);
    }

    #[test]
    fn buy_always_pays_more() {
        let cost = CostModel::new(10.0, 0.0);
        let inst = Instrument::us_equity("SPY");
        let fill = compute_fill(100.0, OrderSide::Buy, 50.0, &inst, &cost);
        assert!(fill.price >= 100.0);
    }

    #[test]
    fn sell_always_receives_less() {
        let cost = CostModel::new(10.0, 0.0);
        let inst = Instrument::us_equity("SPY");
        let fill = compute_fill(100.0, OrderSide::Sell, 50.0, &inst, &cost);
        assert!(fill.price <= 100.0);
    }
}
