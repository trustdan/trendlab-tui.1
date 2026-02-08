//! Order book state machine — manages order lifecycle, OCO groups, and brackets.
//!
//! The order book is the central registry for all orders. It manages:
//! - Order storage and lookup (active + historical)
//! - State transitions (Pending → Triggered → Filled / Cancelled / Expired)
//! - OCO enforcement (one fill cancels all siblings)
//! - Bracket activation (children activate only after entry fills)
//! - Atomic cancel/replace (no "stopless window")
//! - Audit trail for every state transition
//!
//! The order book does NOT compute fill prices or apply slippage — that is the
//! execution engine's job (Phase 7). The order book tracks order state only.

use crate::domain::{
    BracketOrder, OcoGroup, OcoGroupId, Order, OrderAuditEntry, OrderId, OrderStatus, OrderType,
};
use std::collections::HashMap;
use thiserror::Error;

/// Errors from order book operations.
#[derive(Debug, Error)]
pub enum OrderBookError {
    #[error("order {0} not found")]
    OrderNotFound(OrderId),

    #[error("order {0} is not active (status: {1})")]
    OrderNotActive(OrderId, String),

    #[error("invalid transition for order {0}: {1} → {2}")]
    InvalidTransition(OrderId, String, String),

    #[error("order {0} is dormant (bracket entry not yet filled)")]
    OrderIsDormant(OrderId),
}

/// The order book: stores all orders and manages their lifecycle.
///
/// Orders transition through states: Pending → Triggered → Filled / Cancelled / Expired.
/// The book enforces OCO semantics (one fill cancels siblings) and bracket semantics
/// (children activate only after entry fills).
pub struct OrderBook {
    /// All active/historical orders keyed by ID.
    orders: HashMap<OrderId, Order>,

    /// Bracket children waiting for entry fill.
    /// Key: entry order ID. Value: child orders (stop-loss, optional take-profit).
    dormant: HashMap<OrderId, Vec<Order>>,

    /// Bracket relationships: entry_id → BracketOrder.
    brackets: HashMap<OrderId, BracketOrder>,

    /// OCO groups: group_id → OcoGroup.
    oco_groups: HashMap<OcoGroupId, OcoGroup>,

    /// Complete audit trail of every state transition.
    audit_trail: Vec<OrderAuditEntry>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            dormant: HashMap::new(),
            brackets: HashMap::new(),
            oco_groups: HashMap::new(),
            audit_trail: Vec::new(),
        }
    }

    // ── Public API ─────────────────────────────────────────────────────

    /// Look up an order by ID.
    pub fn get_order(&self, id: OrderId) -> Option<&Order> {
        self.orders.get(&id)
    }

    /// Submit a standalone order. It must have status Pending.
    pub fn submit(&mut self, order: Order) {
        debug_assert!(
            order.status == OrderStatus::Pending,
            "submitted order must be Pending"
        );
        self.orders.insert(order.id, order);
    }

    /// Submit a bracket order group: entry + stop_loss + optional take_profit.
    ///
    /// The entry order is placed immediately (Pending). Children are held dormant
    /// until the entry fills, at which point they activate and join the OCO group.
    pub fn submit_bracket(
        &mut self,
        entry: Order,
        stop_loss: Order,
        take_profit: Option<Order>,
        oco_group_id: OcoGroupId,
    ) {
        debug_assert!(entry.status == OrderStatus::Pending);
        debug_assert!(stop_loss.status == OrderStatus::Pending);

        let bracket = BracketOrder {
            entry_id: entry.id,
            stop_loss_id: stop_loss.id,
            take_profit_id: take_profit.as_ref().map(|o| o.id),
            oco_group_id,
        };

        // Build the OCO group from child IDs
        let mut oco_order_ids = vec![stop_loss.id];
        if let Some(ref tp) = take_profit {
            debug_assert!(tp.status == OrderStatus::Pending);
            oco_order_ids.push(tp.id);
        }
        let oco_group = OcoGroup {
            id: oco_group_id,
            order_ids: oco_order_ids,
        };

        // Store children as dormant
        let mut children = vec![stop_loss];
        if let Some(tp) = take_profit {
            children.push(tp);
        }
        self.dormant.insert(entry.id, children);

        // Store bracket and OCO metadata
        self.brackets.insert(entry.id, bracket);
        self.oco_groups.insert(oco_group_id, oco_group);

        // Place entry order
        self.orders.insert(entry.id, entry);
    }

    /// Record a fill on an order, updating filled_quantity and transitioning
    /// to Filled if fully filled. Handles OCO cancellation and bracket activation.
    ///
    /// Returns Ok(true) if the order is now fully filled, Ok(false) if partial.
    pub fn record_fill(
        &mut self,
        order_id: OrderId,
        fill_qty: f64,
        bar_index: usize,
    ) -> Result<bool, OrderBookError> {
        // Validate the order exists and is active
        let order = self
            .orders
            .get(&order_id)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;

        if !order.is_active() {
            return Err(OrderBookError::OrderNotActive(
                order_id,
                format!("{:?}", order.status),
            ));
        }

        // Update filled quantity
        let order = self.orders.get_mut(&order_id).unwrap();
        order.filled_quantity += fill_qty;

        let fully_filled = order.filled_quantity >= order.quantity;
        if fully_filled {
            let from_status = order.status.clone();
            order.status = OrderStatus::Filled;
            self.record_audit(
                order_id,
                from_status,
                OrderStatus::Filled,
                bar_index,
                "filled",
            );
        }

        // OCO cancellation: if this order is in an OCO group and fully filled,
        // cancel all siblings.
        if fully_filled {
            self.handle_oco_cancellation(order_id, bar_index);
        }

        // Bracket activation: if this is a bracket entry and fully filled,
        // activate the dormant children.
        if fully_filled {
            self.activate_bracket_children(order_id, bar_index);
        }

        Ok(fully_filled)
    }

    /// Trigger a stop or stop-limit order: transition from Pending to Triggered.
    pub fn trigger(&mut self, order_id: OrderId, bar_index: usize) -> Result<(), OrderBookError> {
        let order = self
            .orders
            .get(&order_id)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;

        if order.status != OrderStatus::Pending {
            return Err(OrderBookError::InvalidTransition(
                order_id,
                format!("{:?}", order.status),
                "Triggered".into(),
            ));
        }

        // Only stop and stop-limit orders can be triggered
        debug_assert!(
            matches!(
                order.order_type,
                OrderType::StopMarket { .. } | OrderType::StopLimit { .. }
            ),
            "only stop/stop-limit orders can be triggered"
        );

        let from = order.status.clone();
        let order = self.orders.get_mut(&order_id).unwrap();
        order.status = OrderStatus::Triggered;
        self.record_audit(
            order_id,
            from,
            OrderStatus::Triggered,
            bar_index,
            "triggered",
        );
        Ok(())
    }

    /// Cancel an order with a reason.
    ///
    /// Cancellation semantics per state:
    /// - Pending: cancelled cleanly (full quantity cancelled)
    /// - Triggered: cancelled cleanly (unfilled quantity cancelled)
    /// - Filled/Cancelled/Expired: error (terminal states)
    ///
    /// If this is a bracket entry being cancelled, also cleans up dormant children.
    pub fn cancel(
        &mut self,
        order_id: OrderId,
        bar_index: usize,
        reason: &str,
    ) -> Result<(), OrderBookError> {
        let order = self
            .orders
            .get(&order_id)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;

        if !order.is_active() {
            return Err(OrderBookError::OrderNotActive(
                order_id,
                format!("{:?}", order.status),
            ));
        }

        let from = order.status.clone();
        let new_status = OrderStatus::Cancelled {
            reason: reason.to_string(),
        };
        let order = self.orders.get_mut(&order_id).unwrap();
        order.status = new_status.clone();
        self.record_audit(order_id, from, new_status, bar_index, reason);

        // If this was a bracket entry, clean up dormant children
        if let Some(children) = self.dormant.remove(&order_id) {
            for child in children {
                // Record cancelled dormant children in the orders map for audit trail
                let child_id = child.id;
                let mut cancelled_child = child;
                cancelled_child.status = OrderStatus::Cancelled {
                    reason: "bracket entry cancelled".to_string(),
                };
                self.orders.insert(child_id, cancelled_child);
                self.record_audit(
                    child_id,
                    OrderStatus::Pending,
                    OrderStatus::Cancelled {
                        reason: "bracket entry cancelled".to_string(),
                    },
                    bar_index,
                    "bracket entry cancelled",
                );
            }
        }

        Ok(())
    }

    /// Atomic cancel/replace: cancel the old order and submit the new one
    /// in a single operation. No intermediate state where the position is unprotected.
    ///
    /// If the old order was partially filled, only the unfilled remainder is cancelled.
    /// The new order inherits the old order's OCO group membership (if any).
    pub fn cancel_replace(
        &mut self,
        old_id: OrderId,
        mut new_order: Order,
        bar_index: usize,
    ) -> Result<(), OrderBookError> {
        let old_order = self
            .orders
            .get(&old_id)
            .ok_or(OrderBookError::OrderNotFound(old_id))?;

        if !old_order.is_active() {
            return Err(OrderBookError::OrderNotActive(
                old_id,
                format!("{:?}", old_order.status),
            ));
        }

        // Inherit OCO group from old order
        let oco_group_id = old_order.oco_group_id;
        new_order.oco_group_id = oco_group_id;

        // If old order was partially filled, new order quantity = remaining
        let remaining = old_order.remaining_quantity();
        if old_order.filled_quantity > 0.0 {
            new_order.quantity = remaining;
        }

        // Cancel old order
        let from = old_order.status.clone();
        let cancel_status = OrderStatus::Cancelled {
            reason: "replaced".to_string(),
        };
        let old_order = self.orders.get_mut(&old_id).unwrap();
        old_order.status = cancel_status.clone();
        self.record_audit(old_id, from, cancel_status, bar_index, "replaced");

        // Update OCO group membership: swap old ID for new ID
        if let Some(group_id) = oco_group_id {
            if let Some(group) = self.oco_groups.get_mut(&group_id) {
                for id in &mut group.order_ids {
                    if *id == old_id {
                        *id = new_order.id;
                        break;
                    }
                }
            }
        }

        // Submit replacement
        self.orders.insert(new_order.id, new_order);

        Ok(())
    }

    /// Expire an order (e.g., day order at end of bar).
    pub fn expire(&mut self, order_id: OrderId, bar_index: usize) -> Result<(), OrderBookError> {
        let order = self
            .orders
            .get(&order_id)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;

        if !order.is_active() {
            return Err(OrderBookError::OrderNotActive(
                order_id,
                format!("{:?}", order.status),
            ));
        }

        let from = order.status.clone();
        let order = self.orders.get_mut(&order_id).unwrap();
        order.status = OrderStatus::Expired;
        self.record_audit(order_id, from, OrderStatus::Expired, bar_index, "expired");
        Ok(())
    }

    /// Get an order by ID (from active or historical orders).
    pub fn get(&self, id: OrderId) -> Option<&Order> {
        self.orders.get(&id)
    }

    /// Get all active orders (Pending or Triggered).
    pub fn active_orders(&self) -> Vec<&Order> {
        self.orders.values().filter(|o| o.is_active()).collect()
    }

    /// Get active orders for a specific symbol.
    pub fn active_orders_for_symbol(&self, symbol: &str) -> Vec<&Order> {
        self.orders
            .values()
            .filter(|o| o.is_active() && o.symbol == symbol)
            .collect()
    }

    /// Get the bracket info for an entry order (if it's a bracket entry).
    pub fn get_bracket(&self, entry_id: OrderId) -> Option<&BracketOrder> {
        self.brackets.get(&entry_id)
    }

    /// Get the OCO group by ID.
    pub fn get_oco_group(&self, group_id: OcoGroupId) -> Option<&OcoGroup> {
        self.oco_groups.get(&group_id)
    }

    /// Register a standalone OCO group (not part of a bracket).
    ///
    /// Use this when two orders are OCO-linked but not created via `submit_bracket`.
    /// The orders must already be submitted and have their `oco_group_id` set.
    pub fn register_oco_group(&mut self, group: OcoGroup) {
        self.oco_groups.insert(group.id, group);
    }

    /// Get the full audit trail.
    pub fn audit_trail(&self) -> &[OrderAuditEntry] {
        &self.audit_trail
    }

    /// Whether there are any active orders.
    pub fn has_active_orders(&self) -> bool {
        self.orders.values().any(|o| o.is_active())
    }

    /// Count of active orders.
    pub fn active_count(&self) -> usize {
        self.orders.values().filter(|o| o.is_active()).count()
    }

    /// Whether a given order is dormant (bracket child waiting for entry fill).
    pub fn is_dormant(&self, order_id: OrderId) -> bool {
        self.dormant
            .values()
            .any(|children| children.iter().any(|c| c.id == order_id))
    }

    // ── Internal helpers ───────────────────────────────────────────────

    /// Handle OCO cancellation: when an order fills, cancel all siblings in the
    /// same OCO group.
    fn handle_oco_cancellation(&mut self, filled_order_id: OrderId, bar_index: usize) {
        // Find the OCO group this order belongs to
        let oco_group_id = match self.orders.get(&filled_order_id) {
            Some(order) => order.oco_group_id,
            None => return,
        };

        let group_id = match oco_group_id {
            Some(id) => id,
            None => return,
        };

        let sibling_ids: Vec<OrderId> = match self.oco_groups.get(&group_id) {
            Some(group) => group
                .order_ids
                .iter()
                .filter(|&&id| id != filled_order_id)
                .copied()
                .collect(),
            None => return,
        };

        // Cancel each active sibling
        for sibling_id in sibling_ids {
            if let Some(sibling) = self.orders.get(&sibling_id) {
                if sibling.is_active() {
                    let from = sibling.status.clone();
                    let cancel_status = OrderStatus::Cancelled {
                        reason: "OCO sibling filled".to_string(),
                    };
                    let sibling = self.orders.get_mut(&sibling_id).unwrap();
                    sibling.status = cancel_status.clone();
                    self.record_audit(
                        sibling_id,
                        from,
                        cancel_status,
                        bar_index,
                        "OCO sibling filled",
                    );
                }
            }
        }
    }

    /// Activate bracket children when a bracket entry order fills.
    /// Moves children from dormant storage into the active order book.
    fn activate_bracket_children(&mut self, entry_id: OrderId, bar_index: usize) {
        if let Some(children) = self.dormant.remove(&entry_id) {
            for mut child in children {
                let child_id = child.id;
                child.activated_bar = Some(bar_index);
                self.orders.insert(child_id, child);
                self.record_audit(
                    child_id,
                    OrderStatus::Pending, // dormant → active (still Pending status)
                    OrderStatus::Pending,
                    bar_index,
                    "bracket entry filled — child activated",
                );
            }
        }
    }

    /// Record an audit entry.
    fn record_audit(
        &mut self,
        order_id: OrderId,
        from_status: OrderStatus,
        to_status: OrderStatus,
        bar_index: usize,
        reason: &str,
    ) {
        self.audit_trail.push(OrderAuditEntry {
            order_id,
            bar_index,
            from_status,
            to_status,
            reason: reason.to_string(),
        });
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
    use crate::domain::instrument::OrderSide;

    // ── Test helpers ───────────────────────────────────────────────────

    fn make_order(
        id: u64,
        symbol: &str,
        side: OrderSide,
        order_type: OrderType,
        qty: f64,
    ) -> Order {
        Order {
            id: OrderId(id),
            symbol: symbol.into(),
            side,
            order_type,
            quantity: qty,
            filled_quantity: 0.0,
            status: OrderStatus::Pending,
            created_bar: 0,
            parent_id: None,
            oco_group_id: None,
            activated_bar: None,
        }
    }

    fn moo_buy(id: u64, qty: f64) -> Order {
        make_order(id, "SPY", OrderSide::Buy, OrderType::MarketOnOpen, qty)
    }

    fn moc_sell(id: u64, qty: f64) -> Order {
        make_order(id, "SPY", OrderSide::Sell, OrderType::MarketOnClose, qty)
    }

    fn stop_sell(id: u64, trigger: f64, qty: f64) -> Order {
        make_order(
            id,
            "SPY",
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: trigger,
            },
            qty,
        )
    }

    fn limit_buy(id: u64, limit: f64, qty: f64) -> Order {
        make_order(
            id,
            "SPY",
            OrderSide::Buy,
            OrderType::Limit { limit_price: limit },
            qty,
        )
    }

    fn stop_limit_buy(id: u64, trigger: f64, limit: f64, qty: f64) -> Order {
        make_order(
            id,
            "SPY",
            OrderSide::Buy,
            OrderType::StopLimit {
                trigger_price: trigger,
                limit_price: limit,
            },
            qty,
        )
    }

    // ── Submit and retrieve ────────────────────────────────────────────

    #[test]
    fn submit_and_get() {
        let mut book = OrderBook::new();
        let order = moo_buy(1, 100.0);
        book.submit(order);

        let retrieved = book.get(OrderId(1)).unwrap();
        assert_eq!(retrieved.id, OrderId(1));
        assert_eq!(retrieved.quantity, 100.0);
        assert_eq!(retrieved.status, OrderStatus::Pending);
    }

    #[test]
    fn active_orders_returns_only_active() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));
        book.submit(moo_buy(2, 50.0));

        assert_eq!(book.active_orders().len(), 2);
        assert_eq!(book.active_count(), 2);

        // Fill one
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        assert_eq!(book.active_orders().len(), 1);
        assert_eq!(book.active_count(), 1);
        assert_eq!(book.active_orders()[0].id, OrderId(2));
    }

    #[test]
    fn active_orders_for_symbol_filters() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0)); // SPY
        book.submit(make_order(
            2,
            "QQQ",
            OrderSide::Buy,
            OrderType::MarketOnOpen,
            50.0,
        ));

        assert_eq!(book.active_orders_for_symbol("SPY").len(), 1);
        assert_eq!(book.active_orders_for_symbol("QQQ").len(), 1);
        assert_eq!(book.active_orders_for_symbol("IWM").len(), 0);
    }

    // ── Full fill ──────────────────────────────────────────────────────

    #[test]
    fn fill_moo_order() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));

        let fully_filled = book.record_fill(OrderId(1), 100.0, 0).unwrap();
        assert!(fully_filled);

        let order = book.get(OrderId(1)).unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
        assert_eq!(order.filled_quantity, 100.0);
        assert!(!order.is_active());
    }

    #[test]
    fn fill_moc_order() {
        let mut book = OrderBook::new();
        book.submit(moc_sell(1, 50.0));

        let fully_filled = book.record_fill(OrderId(1), 50.0, 5).unwrap();
        assert!(fully_filled);

        let order = book.get(OrderId(1)).unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn fill_limit_order() {
        let mut book = OrderBook::new();
        book.submit(limit_buy(1, 100.0, 50.0));

        let fully_filled = book.record_fill(OrderId(1), 50.0, 3).unwrap();
        assert!(fully_filled);

        let order = book.get(OrderId(1)).unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn fill_immediate_order() {
        let mut book = OrderBook::new();
        book.submit(make_order(
            1,
            "SPY",
            OrderSide::Buy,
            OrderType::MarketImmediate,
            100.0,
        ));

        let fully_filled = book.record_fill(OrderId(1), 100.0, 0).unwrap();
        assert!(fully_filled);
    }

    // ── Stop trigger → fill flow ───────────────────────────────────────

    #[test]
    fn stop_market_trigger_then_fill() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));

        // Initially pending
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Pending);

        // Trigger
        book.trigger(OrderId(1), 5).unwrap();
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Triggered);

        // Fill
        let fully_filled = book.record_fill(OrderId(1), 100.0, 5).unwrap();
        assert!(fully_filled);
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Filled);
    }

    #[test]
    fn stop_limit_trigger_then_fill() {
        let mut book = OrderBook::new();
        book.submit(stop_limit_buy(1, 105.0, 106.0, 100.0));

        book.trigger(OrderId(1), 3).unwrap();
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Triggered);

        book.record_fill(OrderId(1), 100.0, 3).unwrap();
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Filled);
    }

    // ── Partial fills ──────────────────────────────────────────────────

    #[test]
    fn partial_fill_tracking() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));

        let fully_filled = book.record_fill(OrderId(1), 30.0, 0).unwrap();
        assert!(!fully_filled);

        let order = book.get(OrderId(1)).unwrap();
        assert_eq!(order.filled_quantity, 30.0);
        assert_eq!(order.remaining_quantity(), 70.0);
        assert!(order.is_active()); // Still active

        // Complete the fill
        let fully_filled = book.record_fill(OrderId(1), 70.0, 1).unwrap();
        assert!(fully_filled);
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Filled);
    }

    // ── Cancellation ───────────────────────────────────────────────────

    #[test]
    fn cancel_pending_order() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));

        book.cancel(OrderId(1), 0, "user cancel").unwrap();

        let order = book.get(OrderId(1)).unwrap();
        assert_eq!(
            order.status,
            OrderStatus::Cancelled {
                reason: "user cancel".into()
            }
        );
        assert!(!order.is_active());
    }

    #[test]
    fn cancel_triggered_order() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));
        book.trigger(OrderId(1), 3).unwrap();

        book.cancel(OrderId(1), 4, "PM adjustment").unwrap();

        let order = book.get(OrderId(1)).unwrap();
        assert!(matches!(order.status, OrderStatus::Cancelled { .. }));
    }

    #[test]
    fn cancel_filled_order_fails() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        let result = book.cancel(OrderId(1), 1, "too late");
        assert!(result.is_err());
    }

    #[test]
    fn cancel_already_cancelled_fails() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));
        book.cancel(OrderId(1), 0, "first cancel").unwrap();

        let result = book.cancel(OrderId(1), 0, "second cancel");
        assert!(result.is_err());
    }

    // ── Expiration ─────────────────────────────────────────────────────

    #[test]
    fn expire_pending_order() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));

        book.expire(OrderId(1), 0).unwrap();

        let order = book.get(OrderId(1)).unwrap();
        assert_eq!(order.status, OrderStatus::Expired);
        assert!(!order.is_active());
    }

    #[test]
    fn expire_filled_order_fails() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        let result = book.expire(OrderId(1), 0);
        assert!(result.is_err());
    }

    // ── Invalid transitions ────────────────────────────────────────────

    #[test]
    fn trigger_non_pending_fails() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));
        book.trigger(OrderId(1), 0).unwrap();

        // Can't trigger again
        let result = book.trigger(OrderId(1), 1);
        assert!(result.is_err());
    }

    #[test]
    fn fill_nonexistent_order_fails() {
        let mut book = OrderBook::new();
        let result = book.record_fill(OrderId(999), 100.0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn fill_cancelled_order_fails() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));
        book.cancel(OrderId(1), 0, "cancel").unwrap();

        let result = book.record_fill(OrderId(1), 100.0, 0);
        assert!(result.is_err());
    }

    // ── OCO enforcement ────────────────────────────────────────────────

    #[test]
    fn oco_one_fill_cancels_sibling() {
        let mut book = OrderBook::new();

        // Two OCO siblings
        let mut stop = stop_sell(1, 95.0, 100.0);
        stop.oco_group_id = Some(OcoGroupId(10));
        let mut limit = make_order(
            2,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );
        limit.oco_group_id = Some(OcoGroupId(10));

        let oco = OcoGroup {
            id: OcoGroupId(10),
            order_ids: vec![OrderId(1), OrderId(2)],
        };

        book.submit(stop);
        book.submit(limit);
        book.oco_groups.insert(OcoGroupId(10), oco);

        // Trigger and fill the stop
        book.trigger(OrderId(1), 5).unwrap();
        book.record_fill(OrderId(1), 100.0, 5).unwrap();

        // Stop is filled
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Filled);

        // Limit sibling is cancelled
        assert!(matches!(
            book.get(OrderId(2)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
    }

    #[test]
    fn oco_three_siblings_one_fill_cancels_both_others() {
        let mut book = OrderBook::new();

        let mut o1 = moo_buy(1, 100.0);
        o1.oco_group_id = Some(OcoGroupId(20));
        let mut o2 = moo_buy(2, 100.0);
        o2.oco_group_id = Some(OcoGroupId(20));
        let mut o3 = moo_buy(3, 100.0);
        o3.oco_group_id = Some(OcoGroupId(20));

        let oco = OcoGroup {
            id: OcoGroupId(20),
            order_ids: vec![OrderId(1), OrderId(2), OrderId(3)],
        };

        book.submit(o1);
        book.submit(o2);
        book.submit(o3);
        book.oco_groups.insert(OcoGroupId(20), oco);

        // Fill order 2
        book.record_fill(OrderId(2), 100.0, 0).unwrap();

        assert_eq!(book.get(OrderId(2)).unwrap().status, OrderStatus::Filled);
        assert!(matches!(
            book.get(OrderId(1)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
        assert!(matches!(
            book.get(OrderId(3)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
    }

    #[test]
    fn oco_partial_fill_does_not_cancel_siblings() {
        let mut book = OrderBook::new();

        let mut o1 = moo_buy(1, 100.0);
        o1.oco_group_id = Some(OcoGroupId(30));
        let mut o2 = moo_buy(2, 100.0);
        o2.oco_group_id = Some(OcoGroupId(30));

        let oco = OcoGroup {
            id: OcoGroupId(30),
            order_ids: vec![OrderId(1), OrderId(2)],
        };

        book.submit(o1);
        book.submit(o2);
        book.oco_groups.insert(OcoGroupId(30), oco);

        // Partial fill on order 1
        book.record_fill(OrderId(1), 50.0, 0).unwrap();

        // Both still active (partial fill doesn't trigger OCO)
        assert!(book.get(OrderId(1)).unwrap().is_active());
        assert!(book.get(OrderId(2)).unwrap().is_active());
    }

    // ── Bracket orders ─────────────────────────────────────────────────

    #[test]
    fn bracket_children_dormant_before_entry_fill() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let stop = stop_sell(2, 95.0, 100.0);
        let tp = make_order(
            3,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );

        book.submit_bracket(entry, stop, Some(tp), OcoGroupId(50));

        // Entry is active
        assert!(book.get(OrderId(1)).unwrap().is_active());

        // Children are dormant — NOT in active orders
        assert!(book.is_dormant(OrderId(2)));
        assert!(book.is_dormant(OrderId(3)));
        assert!(book.get(OrderId(2)).is_none()); // Not in orders map
        assert!(book.get(OrderId(3)).is_none());

        // Only entry is active
        assert_eq!(book.active_count(), 1);
    }

    #[test]
    fn bracket_children_activate_on_entry_fill() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let mut stop = stop_sell(2, 95.0, 100.0);
        stop.oco_group_id = Some(OcoGroupId(50));
        let mut tp = make_order(
            3,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );
        tp.oco_group_id = Some(OcoGroupId(50));

        book.submit_bracket(entry, stop, Some(tp), OcoGroupId(50));

        // Fill entry
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        // Entry is filled
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Filled);

        // Children are now active
        assert!(!book.is_dormant(OrderId(2)));
        assert!(!book.is_dormant(OrderId(3)));
        assert!(book.get(OrderId(2)).unwrap().is_active());
        assert!(book.get(OrderId(3)).unwrap().is_active());

        // Two active orders (stop + take-profit)
        assert_eq!(book.active_count(), 2);
    }

    #[test]
    fn bracket_oco_works_after_activation() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let mut stop = stop_sell(2, 95.0, 100.0);
        stop.oco_group_id = Some(OcoGroupId(50));
        let mut tp = make_order(
            3,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );
        tp.oco_group_id = Some(OcoGroupId(50));

        book.submit_bracket(entry, stop, Some(tp), OcoGroupId(50));

        // Fill entry → activates children
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        // Now trigger and fill the stop-loss
        book.trigger(OrderId(2), 5).unwrap();
        book.record_fill(OrderId(2), 100.0, 5).unwrap();

        // Stop is filled, take-profit is cancelled (OCO)
        assert_eq!(book.get(OrderId(2)).unwrap().status, OrderStatus::Filled);
        assert!(matches!(
            book.get(OrderId(3)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
        assert_eq!(book.active_count(), 0);
    }

    #[test]
    fn bracket_entry_cancel_cleans_up_dormant_children() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let stop = stop_sell(2, 95.0, 100.0);
        let tp = make_order(
            3,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );

        book.submit_bracket(entry, stop, Some(tp), OcoGroupId(60));

        // Cancel entry
        book.cancel(OrderId(1), 0, "entry cancelled").unwrap();

        // Children should be cleaned up (moved to orders as cancelled)
        assert!(!book.is_dormant(OrderId(2)));
        assert!(!book.is_dormant(OrderId(3)));
        assert!(matches!(
            book.get(OrderId(2)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
        assert!(matches!(
            book.get(OrderId(3)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
        assert_eq!(book.active_count(), 0);
    }

    #[test]
    fn bracket_without_take_profit() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let mut stop = stop_sell(2, 95.0, 100.0);
        stop.oco_group_id = Some(OcoGroupId(70));

        book.submit_bracket(entry, stop, None, OcoGroupId(70));

        // Fill entry
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        // Only stop-loss is active
        assert_eq!(book.active_count(), 1);
        assert!(book.get(OrderId(2)).unwrap().is_active());
    }

    // ── Atomic cancel/replace ──────────────────────────────────────────

    #[test]
    fn cancel_replace_basic() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));

        // Replace with tighter stop
        let replacement = stop_sell(2, 97.0, 100.0);
        book.cancel_replace(OrderId(1), replacement, 5).unwrap();

        // Old order cancelled
        assert!(matches!(
            book.get(OrderId(1)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));

        // New order active
        let new_order = book.get(OrderId(2)).unwrap();
        assert!(new_order.is_active());
        assert_eq!(new_order.quantity, 100.0);
        match new_order.order_type {
            OrderType::StopMarket { trigger_price } => assert_eq!(trigger_price, 97.0),
            _ => panic!("expected StopMarket"),
        }
    }

    #[test]
    fn cancel_replace_with_partial_fill() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));

        // Partial fill of 30 shares
        book.trigger(OrderId(1), 3).unwrap();
        book.record_fill(OrderId(1), 30.0, 3).unwrap();

        // Replace: new order should have qty = 70 (remaining)
        let replacement = stop_sell(2, 97.0, 100.0);
        book.cancel_replace(OrderId(1), replacement, 5).unwrap();

        let new_order = book.get(OrderId(2)).unwrap();
        assert_eq!(new_order.quantity, 70.0); // only unfilled remainder
    }

    #[test]
    fn cancel_replace_inherits_oco_group() {
        let mut book = OrderBook::new();

        let mut stop = stop_sell(1, 95.0, 100.0);
        stop.oco_group_id = Some(OcoGroupId(80));
        let mut tp = make_order(
            2,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );
        tp.oco_group_id = Some(OcoGroupId(80));

        let oco = OcoGroup {
            id: OcoGroupId(80),
            order_ids: vec![OrderId(1), OrderId(2)],
        };

        book.submit(stop);
        book.submit(tp);
        book.oco_groups.insert(OcoGroupId(80), oco);

        // Replace the stop
        let replacement = stop_sell(3, 97.0, 100.0);
        book.cancel_replace(OrderId(1), replacement, 5).unwrap();

        // New order inherits OCO group
        let new_order = book.get(OrderId(3)).unwrap();
        assert_eq!(new_order.oco_group_id, Some(OcoGroupId(80)));

        // OCO group is updated
        let group = book.get_oco_group(OcoGroupId(80)).unwrap();
        assert!(group.order_ids.contains(&OrderId(3)));
        assert!(!group.order_ids.contains(&OrderId(1)));

        // Now fill the take-profit → replacement should be cancelled
        book.record_fill(OrderId(2), 100.0, 6).unwrap();
        assert!(matches!(
            book.get(OrderId(3)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
    }

    #[test]
    fn cancel_replace_filled_order_fails() {
        let mut book = OrderBook::new();
        book.submit(moo_buy(1, 100.0));
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        let replacement = moo_buy(2, 100.0);
        let result = book.cancel_replace(OrderId(1), replacement, 1);
        assert!(result.is_err());
    }

    // ── Audit trail ────────────────────────────────────────────────────

    #[test]
    fn audit_trail_records_all_transitions() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));

        // Trigger
        book.trigger(OrderId(1), 3).unwrap();
        // Fill
        book.record_fill(OrderId(1), 100.0, 5).unwrap();

        let trail = book.audit_trail();
        assert_eq!(trail.len(), 2);

        // First: Pending → Triggered
        assert_eq!(trail[0].order_id, OrderId(1));
        assert_eq!(trail[0].from_status, OrderStatus::Pending);
        assert_eq!(trail[0].to_status, OrderStatus::Triggered);
        assert_eq!(trail[0].bar_index, 3);

        // Second: Triggered → Filled
        assert_eq!(trail[1].order_id, OrderId(1));
        assert_eq!(trail[1].from_status, OrderStatus::Triggered);
        assert_eq!(trail[1].to_status, OrderStatus::Filled);
        assert_eq!(trail[1].bar_index, 5);
    }

    #[test]
    fn audit_trail_includes_oco_cancellations() {
        let mut book = OrderBook::new();

        let mut o1 = moo_buy(1, 100.0);
        o1.oco_group_id = Some(OcoGroupId(90));
        let mut o2 = moo_buy(2, 100.0);
        o2.oco_group_id = Some(OcoGroupId(90));

        let oco = OcoGroup {
            id: OcoGroupId(90),
            order_ids: vec![OrderId(1), OrderId(2)],
        };

        book.submit(o1);
        book.submit(o2);
        book.oco_groups.insert(OcoGroupId(90), oco);

        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        let trail = book.audit_trail();
        // Fill of order 1 + cancel of order 2
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[1].order_id, OrderId(2));
        assert!(matches!(trail[1].to_status, OrderStatus::Cancelled { .. }));
        assert_eq!(trail[1].reason, "OCO sibling filled");
    }

    #[test]
    fn audit_trail_includes_bracket_activation() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let stop = stop_sell(2, 95.0, 100.0);

        book.submit_bracket(entry, stop, None, OcoGroupId(100));
        book.record_fill(OrderId(1), 100.0, 0).unwrap();

        let trail = book.audit_trail();
        // Entry fill + child activation
        assert!(trail.len() >= 2);

        let activation = trail.iter().find(|e| e.order_id == OrderId(2)).unwrap();
        assert_eq!(activation.reason, "bracket entry filled — child activated");
    }

    // ── Empty state ────────────────────────────────────────────────────

    #[test]
    fn new_book_is_empty() {
        let book = OrderBook::new();
        assert!(!book.has_active_orders());
        assert_eq!(book.active_count(), 0);
        assert!(book.audit_trail().is_empty());
    }

    // ── Property-style tests ───────────────────────────────────────────

    /// OCO invariant: siblings can never both fill.
    /// Test with various fill orderings.
    #[test]
    fn oco_invariant_siblings_never_both_fill() {
        for first_to_fill in [OrderId(1), OrderId(2)] {
            let mut book = OrderBook::new();

            let mut o1 = stop_sell(1, 95.0, 100.0);
            o1.oco_group_id = Some(OcoGroupId(200));
            let mut o2 = make_order(
                2,
                "SPY",
                OrderSide::Sell,
                OrderType::Limit { limit_price: 110.0 },
                100.0,
            );
            o2.oco_group_id = Some(OcoGroupId(200));

            let oco = OcoGroup {
                id: OcoGroupId(200),
                order_ids: vec![OrderId(1), OrderId(2)],
            };

            book.submit(o1);
            book.submit(o2);
            book.oco_groups.insert(OcoGroupId(200), oco);

            // Trigger stop if needed
            if first_to_fill == OrderId(1) {
                book.trigger(OrderId(1), 0).unwrap();
            }

            // Fill the first
            book.record_fill(first_to_fill, 100.0, 5).unwrap();

            // Count how many are filled
            let filled_count = [OrderId(1), OrderId(2)]
                .iter()
                .filter(|id| book.get(**id).unwrap().status == OrderStatus::Filled)
                .count();

            assert_eq!(filled_count, 1, "exactly one OCO sibling should be filled");

            // The other must be cancelled
            let other = if first_to_fill == OrderId(1) {
                OrderId(2)
            } else {
                OrderId(1)
            };
            assert!(
                matches!(
                    book.get(other).unwrap().status,
                    OrderStatus::Cancelled { .. }
                ),
                "OCO sibling must be cancelled"
            );
        }
    }

    /// Bracket invariant: children are never active before entry fills.
    #[test]
    fn bracket_invariant_children_never_active_before_entry() {
        let mut book = OrderBook::new();

        let entry = moo_buy(1, 100.0);
        let stop = stop_sell(2, 95.0, 100.0);
        let tp = make_order(
            3,
            "SPY",
            OrderSide::Sell,
            OrderType::Limit { limit_price: 110.0 },
            100.0,
        );

        book.submit_bracket(entry, stop, Some(tp), OcoGroupId(300));

        // Verify children not in active orders at any point before entry fill
        for bar in 0..10 {
            let active_ids: Vec<OrderId> = book.active_orders().iter().map(|o| o.id).collect();
            assert!(
                !active_ids.contains(&OrderId(2)),
                "stop-loss should not be active at bar {bar}"
            );
            assert!(
                !active_ids.contains(&OrderId(3)),
                "take-profit should not be active at bar {bar}"
            );
        }

        // Now fill entry
        book.record_fill(OrderId(1), 100.0, 10).unwrap();

        // Children are now active
        let active_ids: Vec<OrderId> = book.active_orders().iter().map(|o| o.id).collect();
        assert!(active_ids.contains(&OrderId(2)));
        assert!(active_ids.contains(&OrderId(3)));
    }

    /// Cancel/replace atomicity: at no point is the position unprotected.
    /// After cancel_replace, there is always an active replacement order.
    #[test]
    fn cancel_replace_atomicity() {
        let mut book = OrderBook::new();
        book.submit(stop_sell(1, 95.0, 100.0));

        // Before replacement: 1 active stop
        assert_eq!(book.active_orders_for_symbol("SPY").len(), 1);

        // Replace
        let replacement = stop_sell(2, 97.0, 100.0);
        book.cancel_replace(OrderId(1), replacement, 5).unwrap();

        // After replacement: still 1 active stop (the new one)
        let active = book.active_orders_for_symbol("SPY");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, OrderId(2));
    }
}
