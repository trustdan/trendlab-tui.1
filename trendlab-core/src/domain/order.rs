//! Order types, order book state machine types, brackets, and OCO groups.

use super::ids::{OcoGroupId, OrderId};
use super::instrument::OrderSide;
use serde::{Deserialize, Serialize};

/// What kind of order and its price parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    /// Fill at next bar's open price.
    MarketOnOpen,
    /// Fill at bar's close price.
    MarketOnClose,
    /// Fill immediately at current price (intrabar).
    MarketImmediate,
    /// Triggers when price reaches the trigger level, then fills as market.
    StopMarket { trigger_price: f64 },
    /// Fill at limit price or better.
    Limit { limit_price: f64 },
    /// Triggers at trigger_price, then becomes a limit order at limit_price.
    StopLimit {
        trigger_price: f64,
        limit_price: f64,
    },
}

/// Order lifecycle states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Waiting to be triggered or filled.
    Pending,
    /// Stop/stop-limit has triggered, waiting for fill.
    Triggered,
    /// Completely filled.
    Filled,
    /// Cancelled with a reason (OCO sibling filled, user cancel, replace, etc).
    Cancelled { reason: String },
    /// Expired (e.g., day order at end of bar).
    Expired,
}

/// A single order in the order book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub status: OrderStatus,
    pub created_bar: usize,
    /// Parent order ID for bracket children.
    pub parent_id: Option<OrderId>,
    /// OCO group this order belongs to.
    pub oco_group_id: Option<OcoGroupId>,
    /// Bar index when this order was activated (bracket children only).
    /// Used to prevent same-bar entry+exit: children activated during bar T
    /// are not eligible for fill until bar T+1.
    pub activated_bar: Option<usize>,
}

impl Order {
    pub fn remaining_quantity(&self) -> f64 {
        self.quantity - self.filled_quantity
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, OrderStatus::Pending | OrderStatus::Triggered)
    }
}

/// A bracket order: entry + stop-loss + optional take-profit.
///
/// Stop-loss and take-profit children activate only after the entry fills.
/// Children are linked as an OCO group (one-cancels-other).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketOrder {
    pub entry_id: OrderId,
    pub stop_loss_id: OrderId,
    pub take_profit_id: Option<OrderId>,
    pub oco_group_id: OcoGroupId,
}

/// One-cancels-other group: when any member fills, all others are cancelled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcoGroup {
    pub id: OcoGroupId,
    pub order_ids: Vec<OrderId>,
}

/// Audit trail entry for an order state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAuditEntry {
    pub order_id: OrderId,
    pub bar_index: usize,
    pub from_status: OrderStatus,
    pub to_status: OrderStatus,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_remaining_quantity() {
        let order = Order {
            id: OrderId(1),
            symbol: "SPY".into(),
            side: OrderSide::Buy,
            order_type: OrderType::MarketOnOpen,
            quantity: 100.0,
            filled_quantity: 30.0,
            status: OrderStatus::Pending,
            created_bar: 0,
            parent_id: None,
            oco_group_id: None,
            activated_bar: None,
        };
        assert_eq!(order.remaining_quantity(), 70.0);
    }

    #[test]
    fn order_is_active() {
        let mut order = Order {
            id: OrderId(1),
            symbol: "SPY".into(),
            side: OrderSide::Buy,
            order_type: OrderType::StopMarket {
                trigger_price: 105.0,
            },
            quantity: 100.0,
            filled_quantity: 0.0,
            status: OrderStatus::Pending,
            created_bar: 0,
            parent_id: None,
            oco_group_id: None,
            activated_bar: None,
        };
        assert!(order.is_active());

        order.status = OrderStatus::Triggered;
        assert!(order.is_active());

        order.status = OrderStatus::Filled;
        assert!(!order.is_active());

        order.status = OrderStatus::Cancelled {
            reason: "OCO sibling filled".into(),
        };
        assert!(!order.is_active());
    }

    #[test]
    fn order_serialization_roundtrip() {
        let order = Order {
            id: OrderId(42),
            symbol: "AAPL".into(),
            side: OrderSide::Buy,
            order_type: OrderType::StopLimit {
                trigger_price: 150.0,
                limit_price: 151.0,
            },
            quantity: 50.0,
            filled_quantity: 0.0,
            status: OrderStatus::Pending,
            created_bar: 5,
            parent_id: Some(OrderId(41)),
            oco_group_id: Some(OcoGroupId(10)),
            activated_bar: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        let deser: Order = serde_json::from_str(&json).unwrap();
        assert_eq!(order.id, deser.id);
        assert_eq!(order.symbol, deser.symbol);
        assert_eq!(order.quantity, deser.quantity);
    }
}
