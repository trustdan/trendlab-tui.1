use crate::domain::{OrderId, Symbol};
use crate::orders::order::{Order, OrderState};
use crate::orders::order_type::OrderType;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrderBookError {
    #[error("Order {0:?} not found")]
    OrderNotFound(OrderId),

    #[error("Order {0:?} cannot be modified in state {1:?}")]
    InvalidState(OrderId, OrderState),

    #[error("OCO constraint violated: sibling {0:?} already filled")]
    OcoViolation(OrderId),
}

/// OrderBook: manages all orders and their lifecycle
pub struct OrderBook {
    orders: HashMap<OrderId, Order>,
    next_id: u64,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            next_id: 1,
        }
    }

    /// Submit a new order (returns OrderId)
    pub fn submit(
        &mut self,
        symbol: Symbol,
        order_type: OrderType,
        qty: u32,
        bar: usize,
    ) -> OrderId {
        let id = OrderId::from(self.next_id);
        self.next_id += 1;

        let mut order = Order::new(id.clone(), symbol, order_type.clone(), qty, bar);

        // Market orders activate immediately; others stay Pending until explicitly activated
        if matches!(order_type, OrderType::Market(_)) {
            order.activate();
        }

        self.orders.insert(id.clone(), order);
        id
    }

    /// Activate a pending order (e.g., bracket child after parent fills)
    pub fn activate(&mut self, id: OrderId) -> Result<(), OrderBookError> {
        let order = self
            .orders
            .get_mut(&id)
            .ok_or(OrderBookError::OrderNotFound(id.clone()))?;

        if order.state != OrderState::Pending {
            return Err(OrderBookError::InvalidState(id, order.state));
        }

        order.activate();
        Ok(())
    }

    /// Trigger a stop order
    pub fn trigger(&mut self, id: OrderId, bar: usize) -> Result<(), OrderBookError> {
        let order = self
            .orders
            .get_mut(&id)
            .ok_or(OrderBookError::OrderNotFound(id.clone()))?;

        if order.state != OrderState::Active {
            return Err(OrderBookError::InvalidState(id, order.state));
        }

        order.trigger(bar);
        Ok(())
    }

    /// Fill an order (partial or complete)
    /// If OCO sibling exists, cancel it
    pub fn fill(&mut self, id: OrderId, qty: u32, bar: usize) -> Result<(), OrderBookError> {
        // Check OCO constraint BEFORE filling
        let sibling_id = {
            let order = self
                .orders
                .get(&id)
                .ok_or(OrderBookError::OrderNotFound(id.clone()))?;
            order.oco_sibling_id.clone()
        };

        // Fill the order
        {
            let order = self
                .orders
                .get_mut(&id)
                .ok_or(OrderBookError::OrderNotFound(id.clone()))?;
            order.fill(qty, bar);
        }

        // If OCO sibling exists, cancel it
        if let Some(sibling_id) = sibling_id {
            self.cancel(sibling_id, bar)?;
        }

        Ok(())
    }

    /// Cancel an order
    pub fn cancel(&mut self, id: OrderId, bar: usize) -> Result<(), OrderBookError> {
        let order = self
            .orders
            .get_mut(&id)
            .ok_or(OrderBookError::OrderNotFound(id.clone()))?;

        if order.is_terminal() {
            return Err(OrderBookError::InvalidState(id, order.state));
        }

        order.cancel(bar);
        Ok(())
    }

    /// Atomic cancel/replace operation
    /// Cancels old order and submits new one atomically
    pub fn cancel_replace(
        &mut self,
        old_id: OrderId,
        new_order_type: OrderType,
        new_qty: u32,
        bar: usize,
    ) -> Result<OrderId, OrderBookError> {
        // Get old order symbol (before cancelling)
        let symbol = {
            let old_order = self
                .orders
                .get(&old_id)
                .ok_or(OrderBookError::OrderNotFound(old_id.clone()))?;
            old_order.symbol.clone()
        };

        // Cancel old order
        self.cancel(old_id, bar)?;

        // Submit new order
        let new_id = self.submit(symbol, new_order_type, new_qty, bar);

        Ok(new_id)
    }

    /// Set OCO relationship between two orders
    pub fn set_oco(&mut self, id1: OrderId, id2: OrderId) -> Result<(), OrderBookError> {
        // Verify both orders exist
        if !self.orders.contains_key(&id1) {
            return Err(OrderBookError::OrderNotFound(id1));
        }
        if !self.orders.contains_key(&id2) {
            return Err(OrderBookError::OrderNotFound(id2.clone()));
        }

        // Set mutual OCO relationship
        self.orders.get_mut(&id1).unwrap().oco_sibling_id = Some(id2.clone());
        self.orders.get_mut(&id2).unwrap().oco_sibling_id = Some(id1);

        Ok(())
    }

    /// Get all active orders for a symbol
    pub fn active_orders(&self, symbol: &Symbol) -> Vec<&Order> {
        self.orders
            .values()
            .filter(|o| o.symbol == *symbol && o.is_fillable())
            .collect()
    }

    /// Get order by ID
    pub fn get(&self, id: &OrderId) -> Option<&Order> {
        self.orders.get(id)
    }

    /// Get mutable order by ID
    pub fn get_mut(&mut self, id: &OrderId) -> Option<&mut Order> {
        self.orders.get_mut(id)
    }

    /// Get all orders
    pub fn all_orders(&self) -> Vec<&Order> {
        self.orders.values().collect()
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::{MarketTiming, StopDirection};

    #[test]
    fn test_submit_and_activate() {
        let mut book = OrderBook::new();

        let id = book.submit(
            "SPY".to_string(),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            50,
            0,
        );

        let order = book.get(&id).unwrap();
        assert_eq!(order.state, OrderState::Pending);

        book.activate(id.clone()).unwrap();
        let order = book.get(&id).unwrap();
        assert_eq!(order.state, OrderState::Active);
    }

    #[test]
    fn test_oco_cancellation() {
        let mut book = OrderBook::new();

        // Submit stop-loss
        let stop_id = book.submit(
            "SPY".to_string(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            5,
        );

        // Submit take-profit
        let target_id = book.submit(
            "SPY".to_string(),
            OrderType::Limit { limit_price: 105.0 },
            100,
            5,
        );

        // Set OCO relationship
        book.set_oco(stop_id.clone(), target_id.clone()).unwrap();

        // Activate both
        book.activate(stop_id.clone()).unwrap();
        book.activate(target_id.clone()).unwrap();

        // Fill stop-loss
        book.fill(stop_id.clone(), 100, 6).unwrap();

        // Verify stop is filled
        assert_eq!(book.get(&stop_id).unwrap().state, OrderState::Filled);

        // Verify target is cancelled
        assert_eq!(book.get(&target_id).unwrap().state, OrderState::Cancelled);
    }

    #[test]
    fn test_cancel_replace_atomic() {
        let mut book = OrderBook::new();

        let old_id = book.submit(
            "SPY".to_string(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            10,
        );

        book.activate(old_id.clone()).unwrap();

        // Cancel/replace with tighter stop
        let new_id = book
            .cancel_replace(
                old_id.clone(),
                OrderType::StopMarket {
                    direction: StopDirection::Sell,
                    trigger_price: 97.0,
                },
                100,
                12,
            )
            .unwrap();

        // Old order should be cancelled
        assert_eq!(book.get(&old_id).unwrap().state, OrderState::Cancelled);

        // New order should exist and be pending (stop orders start pending)
        assert_eq!(book.get(&new_id).unwrap().state, OrderState::Pending);
        assert_eq!(
            book.get(&new_id).unwrap().order_type.trigger_price(),
            Some(97.0)
        );
    }

    #[test]
    fn test_partial_fill() {
        let mut book = OrderBook::new();

        let id = book.submit(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::Now),
            100,
            0,
        );

        // Market orders activate immediately
        assert_eq!(book.get(&id).unwrap().state, OrderState::Active);

        // Partial fill
        book.fill(id.clone(), 30, 0).unwrap();
        assert_eq!(
            book.get(&id).unwrap().state,
            OrderState::PartiallyFilled { filled_qty: 30 }
        );

        // Complete fill
        book.fill(id.clone(), 70, 0).unwrap();
        assert_eq!(book.get(&id).unwrap().state, OrderState::Filled);
    }

    #[test]
    fn test_active_orders_filter() {
        let mut book = OrderBook::new();

        let id1 = book.submit(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        let id2 = book.submit(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            50,
            0,
        );

        let id3 = book.submit(
            "QQQ".to_string(),
            OrderType::Market(MarketTiming::MOO),
            75,
            0,
        );

        // Fill id2
        book.fill(id2, 50, 0).unwrap();

        // Active orders for SPY should only include id1
        let active_spy = book.active_orders(&"SPY".to_string());
        assert_eq!(active_spy.len(), 1);
        assert_eq!(active_spy[0].id, id1);

        // Active orders for QQQ should include id3
        let active_qqq = book.active_orders(&"QQQ".to_string());
        assert_eq!(active_qqq.len(), 1);
        assert_eq!(active_qqq[0].id, id3);
    }
}
