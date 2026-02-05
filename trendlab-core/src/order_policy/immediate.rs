//! Immediate Order Policy — always use market orders (MOO/MOC/Now)
//!
//! Simplest policy: translate all signals to immediate market orders.
//! Useful for baseline comparisons and simple strategies.

use crate::domain::{Bar, Order, OrderId, OrderSide, OrderState, OrderType, Position};
use crate::order_policy::OrderPolicy;
use crate::signals::SignalIntent;
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for order IDs
static ORDER_COUNTER: AtomicU64 = AtomicU64::new(1000);

fn next_order_id() -> OrderId {
    let id = ORDER_COUNTER.fetch_add(1, Ordering::SeqCst);
    OrderId::new(format!("ord_{}", id))
}

/// Immediate order policy (all market orders)
///
/// # Behavior
/// - Long intent → Market Buy
/// - Short intent → Market Sell
/// - Flat intent → Market exit
#[derive(Debug, Clone)]
pub struct ImmediateOrderPolicy {
    /// Default quantity (will be overridden by sizer)
    default_quantity: f64,
}

impl ImmediateOrderPolicy {
    pub fn new(default_quantity: f64) -> Self {
        assert!(default_quantity > 0.0, "default_quantity must be > 0");
        Self { default_quantity }
    }

    fn create_market_order(&self, symbol: &str, side: OrderSide, quantity: f64) -> Order {
        Order {
            id: next_order_id(),
            symbol: symbol.to_string(),
            side,
            order_type: OrderType::Market,
            quantity,
            state: OrderState::Pending,
        }
    }
}

impl OrderPolicy for ImmediateOrderPolicy {
    fn translate(
        &self,
        intent: SignalIntent,
        current_position: Option<&Position>,
        bar: &Bar,
    ) -> Vec<Order> {
        match (intent, current_position) {
            // Flat intent: exit any existing position
            (SignalIntent::Flat, Some(pos)) => {
                let side = if pos.is_long() {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                vec![self.create_market_order(&bar.symbol, side, pos.quantity.abs())]
            }
            (SignalIntent::Flat, None) => vec![], // Already flat

            // Long intent
            (SignalIntent::Long, Some(pos)) if pos.is_long() => vec![], // Already long
            (SignalIntent::Long, Some(pos)) if pos.is_short() => {
                // Exit short, then enter long
                vec![
                    self.create_market_order(&bar.symbol, OrderSide::Buy, pos.quantity.abs()),
                    self.create_market_order(&bar.symbol, OrderSide::Buy, self.default_quantity),
                ]
            }
            (SignalIntent::Long, None) => {
                vec![self.create_market_order(
                    &bar.symbol,
                    OrderSide::Buy,
                    self.default_quantity,
                )]
            }

            // Short intent
            (SignalIntent::Short, Some(pos)) if pos.is_short() => vec![], // Already short
            (SignalIntent::Short, Some(pos)) if pos.is_long() => {
                // Exit long, then enter short
                vec![
                    self.create_market_order(&bar.symbol, OrderSide::Sell, pos.quantity.abs()),
                    self.create_market_order(&bar.symbol, OrderSide::Sell, self.default_quantity),
                ]
            }
            (SignalIntent::Short, None) => {
                vec![self.create_market_order(
                    &bar.symbol,
                    OrderSide::Sell,
                    self.default_quantity,
                )]
            }

            // Catch-all
            _ => vec![],
        }
    }

    fn name(&self) -> &str {
        "Immediate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar() -> Bar {
        Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            105.0,
            95.0,
            100.0,
            1000000.0,
        )
    }

    #[test]
    fn test_long_intent_creates_market_buy() {
        let policy = ImmediateOrderPolicy::new(100.0);
        let bar = make_bar();

        let orders = policy.translate(SignalIntent::Long, None, &bar);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, OrderSide::Buy);
        assert_eq!(orders[0].order_type, OrderType::Market);
        assert_eq!(orders[0].quantity, 100.0);
    }

    #[test]
    fn test_short_intent_creates_market_sell() {
        let policy = ImmediateOrderPolicy::new(100.0);
        let bar = make_bar();

        let orders = policy.translate(SignalIntent::Short, None, &bar);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, OrderSide::Sell);
        assert_eq!(orders[0].order_type, OrderType::Market);
    }

    #[test]
    fn test_flat_intent_when_no_position() {
        let policy = ImmediateOrderPolicy::new(100.0);
        let bar = make_bar();

        let orders = policy.translate(SignalIntent::Flat, None, &bar);

        assert!(orders.is_empty());
    }
}
