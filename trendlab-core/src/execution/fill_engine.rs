//! Fill engine: orchestrates order fills with realistic execution simulation
//!
//! The fill engine processes orders in three phases per bar:
//! 1. SOB (Start of Bar): Activate day orders, fill MOO orders
//! 2. Intrabar: Trigger and fill based on path policy
//! 3. EOB (End of Bar): Fill MOC orders

use crate::domain::{Bar, OrderId};
use crate::orders::{Order, OrderBook, OrderState, OrderType, MarketTiming};
use super::{
    GapHandler, LiquidityConstraint, PathPolicy, PriorityPolicy, SlippageModel,
};

/// Fill result for a single order
#[derive(Debug, Clone, PartialEq)]
pub struct FillResult {
    pub order_id: OrderId,
    pub fill_qty: u32,
    pub fill_price: f64,
    pub fill_bar: usize,
    pub slippage: f64,
    pub was_gapped: bool,
}

/// Execution engine: processes orders and generates fills
pub struct FillEngine {
    path_policy: Box<dyn PathPolicy>,
    gap_handler: GapHandler,
    slippage_model: Box<dyn SlippageModel>,
    priority_policy: Box<dyn PriorityPolicy>,
    liquidity_constraint: Option<LiquidityConstraint>,
}

impl FillEngine {
    pub fn new(
        path_policy: Box<dyn PathPolicy>,
        gap_handler: GapHandler,
        slippage_model: Box<dyn SlippageModel>,
        priority_policy: Box<dyn PriorityPolicy>,
        liquidity_constraint: Option<LiquidityConstraint>,
    ) -> Self {
        Self {
            path_policy,
            gap_handler,
            slippage_model,
            priority_policy,
            liquidity_constraint,
        }
    }

    /// Process a bar: SOB → Intrabar → EOB
    pub fn process_bar(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Phase 1: Start of Bar (SOB)
        fills.extend(self.process_sob(bar, bar_index, order_book));

        // Phase 2: Intrabar (path-dependent)
        fills.extend(self.process_intrabar(bar, bar_index, order_book));

        // Phase 3: End of Bar (EOB)
        fills.extend(self.process_eob(bar, bar_index, order_book));

        fills
    }

    /// SOB: Activate day orders, fill MOO orders
    fn process_sob(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Activate all pending day orders
        let pending_orders: Vec<OrderId> = order_book
            .all_orders()
            .iter()
            .filter(|o| o.state == OrderState::Pending)
            .map(|o| o.id.clone())
            .collect();

        for id in pending_orders {
            let _ = order_book.activate(id);
        }

        // Fill all active MOO orders at open price
        let moo_orders: Vec<OrderId> = order_book
            .all_orders()
            .iter()
            .filter(|o| {
                o.state == OrderState::Active
                    && matches!(o.order_type, OrderType::Market(MarketTiming::MOO))
            })
            .map(|o| o.id.clone())
            .collect();

        for id in moo_orders {
            if let Some(order) = order_book.get(&id) {
                let qty = order.remaining_qty();
                let base_price = bar.open;
                let slippage = self.slippage_model.compute(&order.order_type, bar, false);
                let fill_price = self.apply_slippage(base_price, slippage, &order.order_type);

                if order_book.fill(id.clone(), qty, bar_index).is_ok() {
                    fills.push(FillResult {
                        order_id: id,
                        fill_qty: qty,
                        fill_price,
                        fill_bar: bar_index,
                        slippage,
                        was_gapped: false,
                    });
                }
            }
        }

        fills
    }

    /// Intrabar: Trigger and fill based on path policy
    fn process_intrabar(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Get active orders (exclude MOC)
        let active_orders: Vec<Order> = order_book
            .all_orders()
            .into_iter()
            .filter(|o| {
                o.state == OrderState::Active
                    && !matches!(o.order_type, OrderType::Market(MarketTiming::MOC))
            })
            .cloned()
            .collect();

        if active_orders.is_empty() {
            return fills;
        }

        // Determine which orders could trigger in this bar
        let triggerable: Vec<Order> = active_orders
            .into_iter()
            .filter(|o| self.can_trigger_in_bar(o, bar))
            .collect();

        if triggerable.is_empty() {
            return fills;
        }

        // Apply path policy to determine trigger sequence
        let trigger_sequence = self.path_policy.order_sequence(&triggerable, bar);

        // Apply priority policy to resolve conflicts
        let prioritized = self.priority_policy.prioritize(trigger_sequence, bar);

        // Process fills in priority order
        for order in prioritized {
            // Check if order still active (OCO may have cancelled it)
            let current_state = order_book.get(&order.id).map(|o| o.state);
            if current_state != Some(OrderState::Active) {
                continue;
            }

            // Trigger order if needed
            if order.order_type.requires_trigger() {
                let _ = order_book.trigger(order.id.clone(), bar_index);
            }

            // Compute fill price (including gap logic)
            let was_gapped = self.gap_handler.did_gap_through(&order, bar);
            let base_price = if was_gapped {
                self.gap_handler.gap_fill_price(bar)
            } else {
                self.get_trigger_or_limit_price(&order, bar)
            };

            let slippage = self.slippage_model.compute(&order.order_type, bar, was_gapped);
            let fill_price = self.apply_slippage(base_price, slippage, &order.order_type);

            // Apply liquidity constraint
            let fill_qty = if let Some(ref liq) = self.liquidity_constraint {
                liq.limit_fill_qty(order.remaining_qty(), bar.volume)
            } else {
                order.remaining_qty()
            };

            // Execute fill
            if order_book.fill(order.id.clone(), fill_qty, bar_index).is_ok() {
                fills.push(FillResult {
                    order_id: order.id.clone(),
                    fill_qty,
                    fill_price,
                    fill_bar: bar_index,
                    slippage,
                    was_gapped,
                });

                // Activate bracket children if this was a parent order fill
                self.activate_bracket_children(&order, order_book);
            }
        }

        fills
    }

    /// EOB: Fill all MOC orders at close price
    fn process_eob(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Fill all active MOC orders at close price
        let moc_orders: Vec<OrderId> = order_book
            .all_orders()
            .iter()
            .filter(|o| {
                o.state == OrderState::Active
                    && matches!(o.order_type, OrderType::Market(MarketTiming::MOC))
            })
            .map(|o| o.id.clone())
            .collect();

        for id in moc_orders {
            if let Some(order) = order_book.get(&id) {
                let qty = order.remaining_qty();
                let base_price = bar.close;
                let slippage = self.slippage_model.compute(&order.order_type, bar, false);
                let fill_price = self.apply_slippage(base_price, slippage, &order.order_type);

                if order_book.fill(id.clone(), qty, bar_index).is_ok() {
                    fills.push(FillResult {
                        order_id: id,
                        fill_qty: qty,
                        fill_price,
                        fill_bar: bar_index,
                        slippage,
                        was_gapped: false,
                    });
                }
            }
        }

        fills
    }

    /// Check if order can trigger in this bar
    fn can_trigger_in_bar(&self, order: &Order, bar: &Bar) -> bool {
        match &order.order_type {
            OrderType::Market(_) => true,
            OrderType::StopMarket { direction, trigger_price } => {
                use crate::orders::StopDirection;
                match direction {
                    StopDirection::Buy => bar.high >= *trigger_price,
                    StopDirection::Sell => bar.low <= *trigger_price,
                }
            }
            OrderType::Limit { limit_price } => {
                // Limit buys can fill at or below limit
                // Limit sells can fill at or above limit
                // For now, assume limit buys when low <= limit, sells when high >= limit
                bar.low <= *limit_price || bar.high >= *limit_price
            }
            OrderType::StopLimit { direction, trigger_price, limit_price } => {
                use crate::orders::StopDirection;
                // Must trigger AND limit must be reachable
                let triggered = match direction {
                    StopDirection::Buy => bar.high >= *trigger_price,
                    StopDirection::Sell => bar.low <= *trigger_price,
                };
                let limit_reached = bar.low <= *limit_price || bar.high >= *limit_price;
                triggered && limit_reached
            }
        }
    }

    /// Get trigger or limit price for order
    fn get_trigger_or_limit_price(&self, order: &Order, bar: &Bar) -> f64 {
        match &order.order_type {
            OrderType::Market(_) => bar.open,
            OrderType::StopMarket { trigger_price, .. } => *trigger_price,
            OrderType::Limit { limit_price } => *limit_price,
            OrderType::StopLimit { limit_price, .. } => *limit_price,
        }
    }

    /// Apply slippage to base price
    fn apply_slippage(&self, base_price: f64, slippage: f64, order_type: &OrderType) -> f64 {
        // Determine direction: buys pay more, sells receive less
        match order_type {
            OrderType::Market(_) => base_price + slippage,
            OrderType::StopMarket { direction, .. } => {
                use crate::orders::StopDirection;
                match direction {
                    StopDirection::Buy => base_price + slippage,
                    StopDirection::Sell => base_price - slippage,
                }
            }
            OrderType::Limit { .. } => base_price, // No slippage for limits
            OrderType::StopLimit { direction, .. } => {
                use crate::orders::StopDirection;
                match direction {
                    StopDirection::Buy => base_price + slippage,
                    StopDirection::Sell => base_price - slippage,
                }
            }
        }
    }

    /// Activate bracket children when parent fills
    fn activate_bracket_children(&self, parent: &Order, order_book: &mut OrderBook) {
        // Find orders that have this order as their parent
        let children: Vec<OrderId> = order_book
            .all_orders()
            .into_iter()
            .filter(|o| o.parent_id == Some(parent.id.clone()))
            .map(|o| o.id.clone())
            .collect();

        for child_id in children {
            let _ = order_book.activate(child_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::StopDirection;
    use crate::execution::{Deterministic, FixedSlippage, WorstCasePriority};

    fn test_bar() -> Bar {
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

    fn test_engine() -> FillEngine {
        FillEngine::new(
            Box::new(Deterministic),
            GapHandler::new(),
            Box::new(FixedSlippage::new(5.0)),
            Box::new(WorstCasePriority),
            None,
        )
    }

    #[test]
    fn test_process_moo_order() {
        let mut engine = test_engine();
        let bar = test_bar();
        let mut order_book = OrderBook::new();

        // Submit MOO order
        let order_id = order_book.submit(
            "SPY".into(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        let fills = engine.process_bar(&bar, 0, &mut order_book);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, order_id);
        assert_eq!(fills[0].fill_qty, 100);
        assert!((fills[0].fill_price - 100.0).abs() < 1.0); // Near open with slippage
    }

    #[test]
    fn test_process_moc_order() {
        let mut engine = test_engine();
        let bar = test_bar();
        let mut order_book = OrderBook::new();

        // Submit MOC order (auto-activates)
        let order_id = order_book.submit(
            "SPY".into(),
            OrderType::Market(MarketTiming::MOC),
            100,
            0,
        );

        let fills = engine.process_bar(&bar, 0, &mut order_book);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, order_id);
        assert_eq!(fills[0].fill_qty, 100);
        assert!((fills[0].fill_price - 101.0).abs() < 1.0); // Near close with slippage
    }

    #[test]
    fn test_stop_order_triggers() {
        let mut engine = test_engine();
        let bar = test_bar();
        let mut order_book = OrderBook::new();

        // Sell stop at 99.0 (will trigger since low=98.0)
        let order_id = order_book.submit(
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 99.0,
            },
            100,
            0,
        );

        let fills = engine.process_bar(&bar, 0, &mut order_book);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, order_id);
        assert_eq!(fills[0].fill_qty, 100);
    }

    #[test]
    fn test_stop_order_no_trigger() {
        let mut engine = test_engine();
        let bar = test_bar();
        let mut order_book = OrderBook::new();

        // Buy stop at 105.0 (won't trigger since high=102.0)
        order_book.submit(
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 105.0,
            },
            100,
            0,
        );

        let fills = engine.process_bar(&bar, 0, &mut order_book);

        assert_eq!(fills.len(), 0); // No fills
    }

    #[test]
    fn test_gap_detection() {
        let mut engine = test_engine();
        let gap_bar = Bar {
            timestamp: chrono::Utc::now(),
            symbol: "SPY".into(),
            open: 95.0,
            high: 95.5,
            low: 94.0, // Gaps below 99.0 trigger
            close: 95.2,
            volume: 1_000_000.0,
        };
        let mut order_book = OrderBook::new();

        // Sell stop at 99.0 (will gap through)
        order_book.submit(
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 99.0,
            },
            100,
            0,
        );

        let fills = engine.process_bar(&gap_bar, 0, &mut order_book);

        assert_eq!(fills.len(), 1);
        assert!(fills[0].was_gapped);
        assert_eq!(fills[0].fill_price, 95.0 - fills[0].slippage); // Filled at open (worse)
    }

    #[test]
    fn test_three_phases_in_sequence() {
        let mut engine = test_engine();
        let bar = test_bar();
        let mut order_book = OrderBook::new();

        // Submit orders for all three phases
        let moo_id = order_book.submit(
            "SPY".into(),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        let stop_id = order_book.submit(
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 99.0,
            },
            100,
            0,
        );

        let moc_id = order_book.submit(
            "SPY".into(),
            OrderType::Market(MarketTiming::MOC),
            100,
            0,
        );

        let fills = engine.process_bar(&bar, 0, &mut order_book);

        // All three orders should fill
        assert_eq!(fills.len(), 3);

        // Verify order IDs
        let fill_ids: Vec<OrderId> = fills.iter().map(|f| f.order_id.clone()).collect();
        assert!(fill_ids.contains(&moo_id));
        assert!(fill_ids.contains(&stop_id));
        assert!(fill_ids.contains(&moc_id));
    }
}
