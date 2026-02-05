use serde::{Deserialize, Serialize};

/// Market order timing variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketTiming {
    /// Market-on-Open: fill at bar open
    MOO,
    /// Market-on-Close: fill at bar close
    MOC,
    /// Market Now: fill immediately at next available price
    Now,
}

/// Stop order direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopDirection {
    Buy,  // trigger when price >= stop
    Sell, // trigger when price <= stop
}

/// Core order type taxonomy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order (various timing)
    Market(MarketTiming),

    /// Stop market: becomes market when price triggers
    StopMarket {
        direction: StopDirection,
        trigger_price: f64,
    },

    /// Limit order: fill only at limit price or better
    Limit { limit_price: f64 },

    /// Stop-limit: becomes limit when stop triggers
    StopLimit {
        direction: StopDirection,
        trigger_price: f64,
        limit_price: f64,
    },
}

impl OrderType {
    /// Check if order type requires a trigger before becoming active
    pub fn requires_trigger(&self) -> bool {
        matches!(
            self,
            OrderType::StopMarket { .. } | OrderType::StopLimit { .. }
        )
    }

    /// Get trigger price if applicable
    pub fn trigger_price(&self) -> Option<f64> {
        match self {
            OrderType::StopMarket { trigger_price, .. } => Some(*trigger_price),
            OrderType::StopLimit { trigger_price, .. } => Some(*trigger_price),
            _ => None,
        }
    }

    /// Get limit price if applicable
    pub fn limit_price(&self) -> Option<f64> {
        match self {
            OrderType::Limit { limit_price } => Some(*limit_price),
            OrderType::StopLimit { limit_price, .. } => Some(*limit_price),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_market_requires_trigger() {
        let order = OrderType::StopMarket {
            direction: StopDirection::Buy,
            trigger_price: 100.0,
        };
        assert!(order.requires_trigger());
        assert_eq!(order.trigger_price(), Some(100.0));
    }

    #[test]
    fn test_market_no_trigger() {
        let order = OrderType::Market(MarketTiming::MOO);
        assert!(!order.requires_trigger());
        assert_eq!(order.trigger_price(), None);
    }

    #[test]
    fn test_stop_limit_has_both_prices() {
        let order = OrderType::StopLimit {
            direction: StopDirection::Sell,
            trigger_price: 95.0,
            limit_price: 94.0,
        };
        assert_eq!(order.trigger_price(), Some(95.0));
        assert_eq!(order.limit_price(), Some(94.0));
    }

    #[test]
    fn test_limit_order_has_limit_price() {
        let order = OrderType::Limit { limit_price: 100.5 };
        assert!(!order.requires_trigger());
        assert_eq!(order.limit_price(), Some(100.5));
        assert_eq!(order.trigger_price(), None);
    }
}
