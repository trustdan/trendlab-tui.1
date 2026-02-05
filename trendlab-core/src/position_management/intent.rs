/// Position management order intents
///
/// PM strategies emit *intents* (cancel/replace requests), never direct fills.
/// The execution engine processes these intents through the order book.
use crate::domain::OrderId;
use crate::orders::Order;
use crate::position_management::manager::Side;

/// Order intent emitted by position management strategies
#[derive(Debug, Clone, PartialEq)]
pub enum OrderIntent {
    /// No action needed
    None,

    /// Place a new order
    Place(Order),

    /// Cancel an existing order
    Cancel { order_id: OrderId },

    /// Cancel and replace an existing order (atomic operation)
    CancelReplace(CancelReplaceIntent),

    /// Update stop price (cancel old, place new)
    UpdateStop {
        old_order_id: OrderId,
        new_stop_price: f64,
        qty: u32,
        side: Side,
    },
}

/// Cancel-replace intent (atomic order modification)
///
/// Used when a PM strategy wants to modify an existing order
/// (e.g., tighten a stop loss). This is atomic to prevent fills
/// during the replacement window.
#[derive(Debug, Clone, PartialEq)]
pub struct CancelReplaceIntent {
    /// Order to cancel
    pub cancel_order_id: OrderId,

    /// New order to place (if cancel succeeds)
    pub new_order: Order,
}

impl OrderIntent {
    /// Check if this is a no-op intent
    pub fn is_none(&self) -> bool {
        matches!(self, OrderIntent::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::MarketTiming;
    use crate::orders::OrderType;

    #[test]
    fn test_order_intent_none() {
        let intent = OrderIntent::None;
        assert!(intent.is_none());
    }

    #[test]
    fn test_update_stop_intent() {
        let old_id = OrderId::from(1);
        let intent = OrderIntent::UpdateStop {
            old_order_id: old_id.clone(),
            new_stop_price: 105.0,
            qty: 10,
            side: Side::Long,
        };

        match intent {
            OrderIntent::UpdateStop {
                old_order_id,
                new_stop_price,
                qty,
                side,
            } => {
                assert_eq!(old_order_id, old_id);
                assert_eq!(new_stop_price, 105.0);
                assert_eq!(qty, 10);
                assert_eq!(side, Side::Long);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_cancel_intent() {
        let order_id = OrderId::from(1);
        let intent = OrderIntent::Cancel {
            order_id: order_id.clone(),
        };

        match intent {
            OrderIntent::Cancel { order_id: id } => {
                assert_eq!(id, order_id);
            }
            _ => panic!("Expected Cancel intent"),
        }
    }

    #[test]
    fn test_place_intent() {
        let order = Order::new(
            OrderId::from(1),
            "AAPL".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );
        let intent = OrderIntent::Place(order.clone());

        match intent {
            OrderIntent::Place(o) => {
                assert_eq!(o.symbol, order.symbol);
                assert_eq!(o.qty, order.qty);
            }
            _ => panic!("Expected Place intent"),
        }
    }

    #[test]
    fn test_cancel_replace_intent() {
        let old_id = OrderId::from(1);
        let new_order = Order::new(
            OrderId::from(2),
            "AAPL".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        let intent = OrderIntent::CancelReplace(CancelReplaceIntent {
            cancel_order_id: old_id.clone(),
            new_order: new_order.clone(),
        });

        match intent {
            OrderIntent::CancelReplace(cr) => {
                assert_eq!(cr.cancel_order_id, old_id);
                assert_eq!(cr.new_order.symbol, new_order.symbol);
            }
            _ => panic!("Expected CancelReplace intent"),
        }
    }
}
