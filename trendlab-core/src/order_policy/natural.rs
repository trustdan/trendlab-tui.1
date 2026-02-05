//! Natural Order Policy — matches order types to signal families
//!
//! Maps signal families to their "natural" order types:
//! - Breakout → StopMarket (enter on continuation)
//! - MeanReversion → Limit (enter on pullback)
//! - Trend → Market (immediate entry)

use crate::domain::{Bar, Order, OrderId, OrderSide, OrderState, OrderType, Position};
use crate::order_policy::OrderPolicy;
use crate::signals::{SignalFamily, SignalIntent};
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for order IDs
static ORDER_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_order_id() -> OrderId {
    let id = ORDER_COUNTER.fetch_add(1, Ordering::SeqCst);
    OrderId::new(format!("ord_{}", id))
}

/// Natural order policy that matches signal family to appropriate order types
///
/// # Entry Order Types
/// - **Breakout**: StopMarket above/below breakout level
/// - **MeanReversion**: Limit at favorable price
/// - **Trend**: Market order for immediate entry
/// - **Other**: Market order (default)
///
/// # Exit Order Types
/// - Always use Market orders for exits (simplicity)
#[derive(Debug, Clone)]
pub struct NaturalOrderPolicy {
    signal_family: SignalFamily,
    /// Default quantity (will be overridden by sizer)
    default_quantity: f64,
}

impl NaturalOrderPolicy {
    pub fn new(signal_family: SignalFamily, default_quantity: f64) -> Self {
        assert!(default_quantity > 0.0, "default_quantity must be > 0");
        Self {
            signal_family,
            default_quantity,
        }
    }

    /// Create entry order based on signal family
    fn create_entry_order(
        &self,
        symbol: &str,
        side: OrderSide,
        bar: &Bar,
        quantity: f64,
    ) -> Order {
        let order_type = match self.signal_family {
            SignalFamily::Breakout => {
                // Enter above/below recent high/low with stop
                let stop_price = match side {
                    OrderSide::Buy => bar.high * 1.001, // Slightly above high
                    OrderSide::Sell => bar.low * 0.999, // Slightly below low
                };
                OrderType::StopMarket { stop_price }
            }
            SignalFamily::MeanReversion => {
                // Enter at favorable price with limit
                let limit_price = match side {
                    OrderSide::Buy => bar.close * 0.999, // Buy slightly below
                    OrderSide::Sell => bar.close * 1.001, // Sell slightly above
                };
                OrderType::Limit { limit_price }
            }
            SignalFamily::Trend | SignalFamily::Other => {
                // Immediate market entry
                OrderType::Market
            }
        };

        Order {
            id: next_order_id(),
            symbol: symbol.to_string(),
            side,
            order_type,
            quantity,
            state: OrderState::Pending,
        }
    }

    /// Create exit order (always market)
    fn create_exit_order(&self, symbol: &str, position: &Position) -> Order {
        let side = if position.is_long() {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };

        Order {
            id: next_order_id(),
            symbol: symbol.to_string(),
            side,
            order_type: OrderType::Market,
            quantity: position.quantity.abs(),
            state: OrderState::Pending,
        }
    }
}

impl OrderPolicy for NaturalOrderPolicy {
    fn translate(
        &self,
        intent: SignalIntent,
        current_position: Option<&Position>,
        bar: &Bar,
    ) -> Vec<Order> {
        match (intent, current_position) {
            // Flat intent: exit any existing position
            (SignalIntent::Flat, Some(pos)) => vec![self.create_exit_order(&bar.symbol, pos)],
            (SignalIntent::Flat, None) => vec![], // Already flat

            // Long intent
            (SignalIntent::Long, Some(pos)) if pos.is_long() => vec![], // Already long
            (SignalIntent::Long, Some(pos)) if pos.is_short() => {
                // Exit short, then enter long
                vec![
                    self.create_exit_order(&bar.symbol, pos),
                    self.create_entry_order(
                        &bar.symbol,
                        OrderSide::Buy,
                        bar,
                        self.default_quantity,
                    ),
                ]
            }
            (SignalIntent::Long, None) => {
                // Enter long from flat
                vec![self.create_entry_order(
                    &bar.symbol,
                    OrderSide::Buy,
                    bar,
                    self.default_quantity,
                )]
            }

            // Short intent
            (SignalIntent::Short, Some(pos)) if pos.is_short() => vec![], // Already short
            (SignalIntent::Short, Some(pos)) if pos.is_long() => {
                // Exit long, then enter short
                vec![
                    self.create_exit_order(&bar.symbol, pos),
                    self.create_entry_order(
                        &bar.symbol,
                        OrderSide::Sell,
                        bar,
                        self.default_quantity,
                    ),
                ]
            }
            (SignalIntent::Short, None) => {
                // Enter short from flat
                vec![self.create_entry_order(
                    &bar.symbol,
                    OrderSide::Sell,
                    bar,
                    self.default_quantity,
                )]
            }

            // Catch-all (should be unreachable but satisfies exhaustiveness)
            _ => vec![],
        }
    }

    fn name(&self) -> &str {
        "Natural"
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

    fn make_long_position() -> Position {
        Position {
            symbol: "SPY".into(),
            quantity: 100.0,
            avg_entry_price: 98.0,
        }
    }

    #[test]
    fn test_breakout_uses_stop_entry() {
        let policy = NaturalOrderPolicy::new(SignalFamily::Breakout, 100.0);
        let bar = make_bar();

        let orders = policy.translate(SignalIntent::Long, None, &bar);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, OrderSide::Buy);
        assert!(matches!(
            orders[0].order_type,
            OrderType::StopMarket { .. }
        ));
    }

    #[test]
    fn test_mean_reversion_uses_limit_entry() {
        let policy = NaturalOrderPolicy::new(SignalFamily::MeanReversion, 100.0);
        let bar = make_bar();

        let orders = policy.translate(SignalIntent::Long, None, &bar);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, OrderSide::Buy);
        assert!(matches!(orders[0].order_type, OrderType::Limit { .. }));
    }

    #[test]
    fn test_trend_uses_market_entry() {
        let policy = NaturalOrderPolicy::new(SignalFamily::Trend, 100.0);
        let bar = make_bar();

        let orders = policy.translate(SignalIntent::Long, None, &bar);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, OrderSide::Buy);
        assert_eq!(orders[0].order_type, OrderType::Market);
    }

    #[test]
    fn test_flat_intent_exits_position() {
        let policy = NaturalOrderPolicy::new(SignalFamily::Trend, 100.0);
        let bar = make_bar();
        let pos = make_long_position();

        let orders = policy.translate(SignalIntent::Flat, Some(&pos), &bar);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, OrderSide::Sell);
        assert_eq!(orders[0].order_type, OrderType::Market);
    }

    #[test]
    fn test_long_intent_when_already_long() {
        let policy = NaturalOrderPolicy::new(SignalFamily::Trend, 100.0);
        let bar = make_bar();
        let pos = make_long_position();

        let orders = policy.translate(SignalIntent::Long, Some(&pos), &bar);

        assert!(orders.is_empty()); // No action needed
    }

    #[test]
    fn test_reverse_from_short_to_long() {
        let policy = NaturalOrderPolicy::new(SignalFamily::Trend, 100.0);
        let bar = make_bar();
        let mut pos = make_long_position();
        pos.quantity = -100.0; // Short position

        let orders = policy.translate(SignalIntent::Long, Some(&pos), &bar);

        assert_eq!(orders.len(), 2); // Exit + Entry
        assert_eq!(orders[0].side, OrderSide::Buy); // Cover short
        assert_eq!(orders[1].side, OrderSide::Buy); // Enter long
    }
}
