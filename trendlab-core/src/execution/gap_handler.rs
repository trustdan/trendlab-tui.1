//! Gap handling: detect when stop orders gap through their triggers
//!
//! Gap rule: If a stop is gapped through (price gaps past the trigger),
//! the fill occurs at the open price (worse) rather than the trigger price.

use crate::domain::Bar;
use crate::orders::{Order, OrderType, StopDirection};

/// Gap handler: detects when orders gap through triggers
#[derive(Debug, Clone, Copy)]
pub struct GapHandler;

impl GapHandler {
    pub fn new() -> Self {
        Self
    }

    /// Check if this order was gapped through in this bar
    pub fn did_gap_through(&self, order: &Order, bar: &Bar) -> bool {
        match &order.order_type {
            OrderType::StopMarket { direction, trigger_price } => {
                self.check_stop_gap(*direction, *trigger_price, bar)
            }
            OrderType::StopLimit { direction, trigger_price, .. } => {
                self.check_stop_gap(*direction, *trigger_price, bar)
            }
            _ => false, // Market and Limit orders cannot gap
        }
    }

    /// Check if a stop order gaps through its trigger
    fn check_stop_gap(&self, direction: StopDirection, trigger: f64, bar: &Bar) -> bool {
        match direction {
            StopDirection::Buy => {
                // Buy stop: triggers when price rises to trigger
                // Gapped if open is already above trigger (gap up through trigger)
                bar.open > trigger && bar.low > trigger
            }
            StopDirection::Sell => {
                // Sell stop: triggers when price falls to trigger
                // Gapped if open is already below trigger (gap down through trigger)
                bar.open < trigger && bar.high < trigger
            }
        }
    }

    /// Get the fill price for a gapped order (always at open, which is worse)
    pub fn gap_fill_price(&self, bar: &Bar) -> f64 {
        bar.open
    }
}

impl Default for GapHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OrderId;
    use crate::orders::Order;

    fn test_bar_gap_up() -> Bar {
        Bar {
            timestamp: chrono::Utc::now(),
            symbol: "SPY".into(),
            open: 105.0,
            high: 106.0,
            low: 104.5, // Gaps above 100 trigger
            close: 105.5,
            volume: 1_000_000.0,
        }
    }

    fn test_bar_gap_down() -> Bar {
        Bar {
            timestamp: chrono::Utc::now(),
            symbol: "SPY".into(),
            open: 95.0,
            high: 95.5, // Gaps below 100 trigger
            low: 94.0,
            close: 95.2,
            volume: 1_000_000.0,
        }
    }

    fn test_bar_no_gap() -> Bar {
        Bar {
            timestamp: chrono::Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 102.0,
            low: 98.0,
            close: 101.0,
            volume: 1_000_000.0,
        }
    }

    #[test]
    fn test_buy_stop_gaps_through() {
        let handler = GapHandler::new();
        let bar = test_bar_gap_up();

        let order = Order::new(
            OrderId::from(1),
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            100,
            0,
        );

        assert!(handler.did_gap_through(&order, &bar));
        assert_eq!(handler.gap_fill_price(&bar), 105.0);
    }

    #[test]
    fn test_sell_stop_gaps_through() {
        let handler = GapHandler::new();
        let bar = test_bar_gap_down();

        let order = Order::new(
            OrderId::from(1),
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 100.0,
            },
            100,
            0,
        );

        assert!(handler.did_gap_through(&order, &bar));
        assert_eq!(handler.gap_fill_price(&bar), 95.0);
    }

    #[test]
    fn test_no_gap_normal_trigger() {
        let handler = GapHandler::new();
        let bar = test_bar_no_gap();

        let order = Order::new(
            OrderId::from(1),
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 99.0,
            },
            100,
            0,
        );

        assert!(!handler.did_gap_through(&order, &bar));
    }

    #[test]
    fn test_market_order_cannot_gap() {
        let handler = GapHandler::new();
        let bar = test_bar_gap_up();

        let order = Order::new(
            OrderId::from(1),
            "SPY".into(),
            OrderType::Market(crate::orders::MarketTiming::MOO),
            100,
            0,
        );

        assert!(!handler.did_gap_through(&order, &bar));
    }

    #[test]
    fn test_limit_order_cannot_gap() {
        let handler = GapHandler::new();
        let bar = test_bar_gap_up();

        let order = Order::new(
            OrderId::from(1),
            "SPY".into(),
            OrderType::Limit {
                limit_price: 100.0,
            },
            100,
            0,
        );

        assert!(!handler.did_gap_through(&order, &bar));
    }
}
