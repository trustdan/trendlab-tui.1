//! Slippage models: compute execution cost
//!
//! Slippage represents the difference between expected and actual fill price.
//! - Market orders: always pay slippage (crossing spread + adverse selection)
//! - Stop orders: pay slippage when triggered
//! - Limit orders: zero slippage (passive fill)
//! - Gapped orders: 2x slippage penalty

use crate::domain::Bar;
use crate::orders::OrderType;

/// Slippage model: computes cost added to fill price
pub trait SlippageModel: Send + Sync {
    /// Compute slippage for this order type and market conditions
    ///
    /// # Arguments
    /// - `order_type`: Type of order being filled
    /// - `bar`: Current bar data
    /// - `was_gapped`: True if order gapped through trigger
    ///
    /// # Returns
    /// Slippage in price units (positive = cost)
    fn compute(
        &self,
        order_type: &OrderType,
        bar: &Bar,
        was_gapped: bool,
    ) -> f64;

    /// Name of this model
    fn name(&self) -> &str;
}

/// Fixed slippage: constant cost in basis points or absolute dollars
#[derive(Debug, Clone, Copy)]
pub struct FixedSlippage {
    /// Slippage in basis points (e.g., 5 = 0.05%)
    pub bps: f64,
    /// Optional absolute slippage (e.g., $0.01)
    pub absolute: Option<f64>,
}

impl FixedSlippage {
    pub fn new(bps: f64) -> Self {
        Self {
            bps,
            absolute: None,
        }
    }

    pub fn with_absolute(bps: f64, absolute: f64) -> Self {
        Self {
            bps,
            absolute: Some(absolute),
        }
    }
}

impl SlippageModel for FixedSlippage {
    fn compute(&self, order_type: &OrderType, bar: &Bar, was_gapped: bool) -> f64 {
        // Limit orders have zero slippage (passive fill)
        if matches!(order_type, OrderType::Limit { .. } | OrderType::StopLimit { .. }) {
            return 0.0;
        }

        // Base slippage
        let base_slip = if let Some(abs) = self.absolute {
            abs
        } else {
            bar.close * (self.bps / 10_000.0)
        };

        // Gapped orders pay 2x slippage (worse execution)
        if was_gapped {
            base_slip * 2.0
        } else {
            base_slip
        }
    }

    fn name(&self) -> &str {
        "FixedSlippage"
    }
}

/// ATR-based slippage: scales with volatility
#[derive(Debug, Clone, Copy)]
pub struct AtrSlippage {
    /// Multiplier of ATR (e.g., 0.1 = 10% of ATR)
    pub atr_multiple: f64,
    /// Cached ATR value (updated externally)
    pub current_atr: f64,
}

impl AtrSlippage {
    pub fn new(atr_multiple: f64, current_atr: f64) -> Self {
        Self {
            atr_multiple,
            current_atr,
        }
    }

    pub fn update_atr(&mut self, new_atr: f64) {
        self.current_atr = new_atr;
    }
}

impl SlippageModel for AtrSlippage {
    fn compute(&self, order_type: &OrderType, _bar: &Bar, was_gapped: bool) -> f64 {
        // Limit orders have zero slippage
        if matches!(order_type, OrderType::Limit { .. } | OrderType::StopLimit { .. }) {
            return 0.0;
        }

        // Base slippage: fraction of ATR
        let base_slip = self.current_atr * self.atr_multiple;

        // Gapped orders pay 2x slippage
        if was_gapped {
            base_slip * 2.0
        } else {
            base_slip
        }
    }

    fn name(&self) -> &str {
        "AtrSlippage"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::{MarketTiming, StopDirection};

    fn test_bar() -> Bar {
        Bar {
            timestamp: chrono::Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 102.0,
            low: 98.0,
            close: 100.0,
            volume: 1_000_000.0,
        }
    }

    #[test]
    fn test_fixed_slippage_market_order() {
        let model = FixedSlippage::new(5.0); // 5 bps = 0.05%
        let bar = test_bar();
        let order_type = OrderType::Market(MarketTiming::Now);

        let slippage = model.compute(&order_type, &bar, false);
        assert_eq!(slippage, 100.0 * 0.0005); // 0.05 = 5 bps of 100
    }

    #[test]
    fn test_fixed_slippage_gapped_2x() {
        let model = FixedSlippage::new(5.0);
        let bar = test_bar();
        let order_type = OrderType::StopMarket {
            direction: StopDirection::Sell,
            trigger_price: 99.0,
        };

        let slippage = model.compute(&order_type, &bar, true);
        assert_eq!(slippage, 100.0 * 0.0005 * 2.0); // 2x for gap
    }

    #[test]
    fn test_fixed_slippage_limit_zero() {
        let model = FixedSlippage::new(5.0);
        let bar = test_bar();
        let order_type = OrderType::Limit {
            limit_price: 100.0,
        };

        let slippage = model.compute(&order_type, &bar, false);
        assert_eq!(slippage, 0.0); // Limit orders have zero slippage
    }

    #[test]
    fn test_fixed_slippage_absolute() {
        let model = FixedSlippage::with_absolute(0.0, 0.05);
        let bar = test_bar();
        let order_type = OrderType::Market(MarketTiming::Now);

        let slippage = model.compute(&order_type, &bar, false);
        assert_eq!(slippage, 0.05); // Absolute $0.05
    }

    #[test]
    fn test_atr_slippage_normal() {
        let model = AtrSlippage::new(0.1, 2.0); // 10% of ATR=2.0
        let bar = test_bar();
        let order_type = OrderType::Market(MarketTiming::Now);

        let slippage = model.compute(&order_type, &bar, false);
        assert_eq!(slippage, 0.2); // 10% of 2.0 ATR
    }

    #[test]
    fn test_atr_slippage_gapped_2x() {
        let model = AtrSlippage::new(0.1, 2.0);
        let bar = test_bar();
        let order_type = OrderType::StopMarket {
            direction: StopDirection::Buy,
            trigger_price: 101.0,
        };

        let slippage = model.compute(&order_type, &bar, true);
        assert_eq!(slippage, 0.4); // 2x for gap
    }

    #[test]
    fn test_atr_slippage_limit_zero() {
        let model = AtrSlippage::new(0.1, 2.0);
        let bar = test_bar();
        let order_type = OrderType::Limit {
            limit_price: 100.0,
        };

        let slippage = model.compute(&order_type, &bar, false);
        assert_eq!(slippage, 0.0); // Limit orders have zero slippage
    }

    #[test]
    fn test_atr_update() {
        let mut model = AtrSlippage::new(0.1, 2.0);
        assert_eq!(model.current_atr, 2.0);

        model.update_atr(3.0);
        assert_eq!(model.current_atr, 3.0);

        let bar = test_bar();
        let order_type = OrderType::Market(MarketTiming::Now);
        let slippage = model.compute(&order_type, &bar, false);
        assert!((slippage - 0.3).abs() < 1e-10); // 10% of updated 3.0 ATR (floating point tolerance)
    }
}
