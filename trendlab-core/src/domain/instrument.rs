use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tick/lot rounding policy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TickPolicy {
    /// Reject orders that aren't already tick-aligned
    Reject,
    /// Round to nearest tick
    RoundNearest,
    /// Round down (more conservative for buys)
    RoundDown,
    /// Round up (more conservative for sells)
    RoundUp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OrderSideForRounding {
    Buy,
    Sell,
}

/// Instrument metadata for tick size, lot size, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Instrument {
    pub symbol: String,
    pub tick_size: f64,
    pub lot_size: f64,
    pub currency: String,
    pub asset_class: AssetClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AssetClass {
    Equity,
    Future,
    Forex,
    Crypto,
}

impl Instrument {
    /// Create new instrument
    pub fn new(
        symbol: String,
        tick_size: f64,
        lot_size: f64,
        currency: String,
        asset_class: AssetClass,
    ) -> Self {
        Self { symbol, tick_size, lot_size, currency, asset_class }
    }

    /// Round price according to policy
    pub fn round_price(&self, price: f64, policy: TickPolicy) -> f64 {
        let ticks = price / self.tick_size;
        let rounded_ticks = match policy {
            TickPolicy::RoundNearest => ticks.round(),
            TickPolicy::RoundDown => ticks.floor(),
            TickPolicy::RoundUp => ticks.ceil(),
            TickPolicy::Reject => ticks, // will be checked later
        };
        rounded_ticks * self.tick_size
    }

    /// Apply side-aware rounding (buy limits round down, sell limits round up)
    pub fn round_price_side_aware(&self, price: f64, side: OrderSideForRounding) -> f64 {
        let policy = match side {
            OrderSideForRounding::Buy => TickPolicy::RoundDown,
            OrderSideForRounding::Sell => TickPolicy::RoundUp,
        };
        self.round_price(price, policy)
    }

    /// Validate price respects tick size
    pub fn validate_price(&self, price: f64, policy: TickPolicy) -> Result<f64, InstrumentError> {
        let ticks = price / self.tick_size;

        if policy == TickPolicy::Reject {
            // Check if ticks is close to a whole number
            if (ticks - ticks.round()).abs() > 1e-10 {
                return Err(InstrumentError::InvalidTickSize { price, tick_size: self.tick_size });
            }
        }

        let rounded = self.round_price(price, policy);
        Ok(rounded)
    }

    /// Validate quantity respects lot size
    pub fn validate_quantity(&self, qty: f64, policy: TickPolicy) -> Result<f64, InstrumentError> {
        let lots = qty / self.lot_size;

        if policy == TickPolicy::Reject {
            // Check if lots is close to a whole number
            if (lots - lots.round()).abs() > 1e-10 {
                return Err(InstrumentError::InvalidLotSize {
                    quantity: qty,
                    lot_size: self.lot_size,
                });
            }
        }

        let rounded_lots = match policy {
            TickPolicy::RoundNearest => lots.round(),
            TickPolicy::RoundDown => lots.floor(),
            TickPolicy::RoundUp => lots.ceil(),
            TickPolicy::Reject => lots,
        };
        let rounded = rounded_lots * self.lot_size;
        Ok(rounded)
    }
}

#[derive(Debug, Error)]
pub enum InstrumentError {
    #[error("Price {price} does not respect tick_size {tick_size}")]
    InvalidTickSize { price: f64, tick_size: f64 },

    #[error("Quantity {quantity} does not respect lot_size {lot_size}")]
    InvalidLotSize { quantity: f64, lot_size: f64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_size_rounding() {
        let inst = Instrument::new("SPY".into(), 0.01, 1.0, "USD".into(), AssetClass::Equity);
        assert_eq!(inst.round_price(100.126, TickPolicy::RoundNearest), 100.13);
        assert_eq!(inst.round_price(100.124, TickPolicy::RoundNearest), 100.12);
    }

    #[test]
    fn test_side_aware_rounding() {
        let inst = Instrument::new("ES".into(), 0.25, 1.0, "USD".into(), AssetClass::Future);
        // Buy limits round down (more conservative)
        assert_eq!(inst.round_price_side_aware(4500.10, OrderSideForRounding::Buy), 4500.00);
        // Sell limits round up (more conservative)
        assert_eq!(inst.round_price_side_aware(4500.10, OrderSideForRounding::Sell), 4500.25);
    }

    #[test]
    fn test_validate_price_rejects_bad_tick() {
        let inst = Instrument::new("ES".into(), 0.25, 1.0, "USD".into(), AssetClass::Future);
        assert!(inst.validate_price(4500.10, TickPolicy::Reject).is_err());
        assert!(inst.validate_price(4500.25, TickPolicy::Reject).is_ok());
        assert!(inst.validate_price(4500.50, TickPolicy::Reject).is_ok());
    }

    #[test]
    fn test_validate_price_with_rounding_policy() {
        let inst = Instrument::new("ES".into(), 0.25, 1.0, "USD".into(), AssetClass::Future);
        // RoundNearest policy rounds invalid prices
        assert_eq!(inst.validate_price(4500.10, TickPolicy::RoundNearest).unwrap(), 4500.00);
        assert_eq!(inst.validate_price(4500.15, TickPolicy::RoundNearest).unwrap(), 4500.25);
    }

    #[test]
    fn test_validate_quantity_respects_lot_size() {
        let inst = Instrument::new(
            "BTC".into(),
            0.01,
            0.001, // crypto lot size
            "USD".into(),
            AssetClass::Crypto,
        );
        // 1.5 / 0.001 = 1500 (whole number), so this is valid
        assert!(inst.validate_quantity(1.500, TickPolicy::Reject).is_ok());
        // 1.0015 / 0.001 = 1001.5 (not whole number), should fail
        assert!(inst.validate_quantity(1.0015, TickPolicy::Reject).is_err());
        // 1.001 / 0.001 = 1001 (whole number), should pass
        assert!(inst.validate_quantity(1.001, TickPolicy::Reject).is_ok());
        // With rounding policy: 1.0015 / 0.001 = 1001.5 -> rounds to 1002 -> 1.002
        assert_eq!(inst.validate_quantity(1.0015, TickPolicy::RoundNearest).unwrap(), 1.002);
    }
}
