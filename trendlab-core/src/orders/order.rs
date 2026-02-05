use crate::domain::{OrderId, Symbol};
use crate::orders::order_type::OrderType;
use serde::{Deserialize, Serialize};

/// Order lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    /// Pending: not yet active (e.g., bracket child waiting for parent fill)
    Pending,
    /// Active: eligible for triggering/filling
    Active,
    /// Triggered: stop has triggered, now acting as market/limit
    Triggered,
    /// PartiallyFilled: some qty filled, rest still active
    PartiallyFilled { filled_qty: u32 },
    /// Filled: order complete
    Filled,
    /// Cancelled: user or system cancelled
    Cancelled,
    /// Expired: time-based expiry (e.g., day order at end of day)
    Expired,
}

/// An order with full lifecycle tracking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: Symbol,
    pub order_type: OrderType,
    pub qty: u32,
    pub filled_qty: u32,
    pub state: OrderState,

    /// Optional: parent order ID (for bracket children)
    pub parent_id: Option<OrderId>,

    /// Optional: OCO sibling ID (for bracket stop/target pairs)
    pub oco_sibling_id: Option<OrderId>,

    /// Bar number when order was created
    pub created_bar: usize,

    /// Bar number when order was filled/cancelled/expired (if applicable)
    pub closed_bar: Option<usize>,
}

impl Order {
    /// Create a new order in Pending state
    pub fn new(
        id: OrderId,
        symbol: Symbol,
        order_type: OrderType,
        qty: u32,
        created_bar: usize,
    ) -> Self {
        Self {
            id,
            symbol,
            order_type,
            qty,
            filled_qty: 0,
            state: OrderState::Pending,
            parent_id: None,
            oco_sibling_id: None,
            created_bar,
            closed_bar: None,
        }
    }

    /// Activate the order (Pending → Active)
    pub fn activate(&mut self) {
        if self.state == OrderState::Pending {
            self.state = OrderState::Active;
        }
    }

    /// Trigger a stop order (Active → Triggered)
    pub fn trigger(&mut self, _bar: usize) {
        if self.state == OrderState::Active && self.order_type.requires_trigger() {
            self.state = OrderState::Triggered;
        }
    }

    /// Fill the order (partial or complete)
    pub fn fill(&mut self, qty: u32, bar: usize) {
        assert!(qty <= self.remaining_qty(), "Cannot fill more than remaining");

        self.filled_qty += qty;

        if self.filled_qty >= self.qty {
            self.state = OrderState::Filled;
            self.closed_bar = Some(bar);
        } else {
            self.state = OrderState::PartiallyFilled {
                filled_qty: self.filled_qty,
            };
        }
    }

    /// Cancel the order
    pub fn cancel(&mut self, bar: usize) {
        if !self.is_terminal() {
            self.state = OrderState::Cancelled;
            self.closed_bar = Some(bar);
        }
    }

    /// Expire the order (e.g., day order at EOD)
    pub fn expire(&mut self, bar: usize) {
        if !self.is_terminal() {
            self.state = OrderState::Expired;
            self.closed_bar = Some(bar);
        }
    }

    /// Get remaining unfilled quantity
    pub fn remaining_qty(&self) -> u32 {
        self.qty.saturating_sub(self.filled_qty)
    }

    /// Check if order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            OrderState::Filled | OrderState::Cancelled | OrderState::Expired
        )
    }

    /// Check if order is eligible for fill attempts
    pub fn is_fillable(&self) -> bool {
        matches!(
            self.state,
            OrderState::Active | OrderState::Triggered | OrderState::PartiallyFilled { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::{MarketTiming, StopDirection};

    #[test]
    fn test_order_lifecycle_market() {
        let mut order = Order::new(
            OrderId::from(1),
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        assert_eq!(order.state, OrderState::Pending);

        order.activate();
        assert_eq!(order.state, OrderState::Active);

        order.fill(100, 0);
        assert_eq!(order.state, OrderState::Filled);
        assert_eq!(order.filled_qty, 100);
        assert!(order.is_terminal());
    }

    #[test]
    fn test_partial_fill() {
        let mut order = Order::new(
            OrderId::from(2),
            "SPY".to_string(),
            OrderType::Market(MarketTiming::Now),
            100,
            5,
        );

        order.activate();
        order.fill(30, 5);

        assert_eq!(
            order.state,
            OrderState::PartiallyFilled { filled_qty: 30 }
        );
        assert_eq!(order.remaining_qty(), 70);
        assert!(!order.is_terminal());

        order.fill(70, 5);
        assert_eq!(order.state, OrderState::Filled);
    }

    #[test]
    fn test_stop_trigger() {
        let mut order = Order::new(
            OrderId::from(3),
            "SPY".to_string(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            50,
            10,
        );

        order.activate();
        assert_eq!(order.state, OrderState::Active);

        order.trigger(12);
        assert_eq!(order.state, OrderState::Triggered);

        order.fill(50, 12);
        assert_eq!(order.state, OrderState::Filled);
    }

    #[test]
    fn test_cancel_active_order() {
        let mut order = Order::new(
            OrderId::from(4),
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOC),
            100,
            15,
        );

        order.activate();
        order.cancel(16);

        assert_eq!(order.state, OrderState::Cancelled);
        assert_eq!(order.closed_bar, Some(16));
        assert!(order.is_terminal());
    }

    #[test]
    #[should_panic(expected = "Cannot fill more than remaining")]
    fn test_overfill_panics() {
        let mut order = Order::new(
            OrderId::from(5),
            "SPY".to_string(),
            OrderType::Market(MarketTiming::Now),
            50,
            0,
        );

        order.activate();
        order.fill(60, 0); // Should panic
    }

    #[test]
    fn test_remaining_qty() {
        let mut order = Order::new(
            OrderId::from(6),
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        assert_eq!(order.remaining_qty(), 100);

        order.activate();
        order.fill(25, 0);
        assert_eq!(order.remaining_qty(), 75);

        order.fill(75, 0);
        assert_eq!(order.remaining_qty(), 0);
    }
}
