//! Order priority: resolve conflicts when multiple orders could fill
//!
//! When path policy determines trigger sequence, priority policy handles
//! cases where orders conflict (e.g., OCO orders, limited capital).

use crate::domain::Bar;
use crate::orders::Order;

/// Priority policy: resolves conflicts between orders
pub trait PriorityPolicy: Send + Sync {
    /// Prioritize orders (may filter or reorder)
    fn prioritize(&self, orders: Vec<Order>, bar: &Bar) -> Vec<Order>;

    /// Name of this policy
    fn name(&self) -> &str;
}

/// WorstCase priority: stop-losses before take-profits
#[derive(Debug, Clone, Copy)]
pub struct WorstCasePriority;

impl PriorityPolicy for WorstCasePriority {
    fn prioritize(&self, orders: Vec<Order>, _bar: &Bar) -> Vec<Order> {
        let mut result = orders;

        // Prioritize stop-losses over take-profits (conservative)
        result.sort_by_key(|order| {
            if self.is_stop_loss(order) {
                0 // Stops first
            } else if self.is_take_profit(order) {
                2 // Targets last
            } else {
                1 // Others middle
            }
        });

        result
    }

    fn name(&self) -> &str {
        "WorstCasePriority"
    }
}

impl WorstCasePriority {
    fn is_stop_loss(&self, order: &Order) -> bool {
        // Heuristic: has parent (is bracket child) but is not a take-profit
        order.parent_id.is_some() && !self.is_take_profit(order)
    }

    fn is_take_profit(&self, order: &Order) -> bool {
        // Heuristic: has both parent and OCO sibling (bracket target)
        order.parent_id.is_some() && order.oco_sibling_id.is_some()
    }
}

/// BestCase priority: take-profits before stop-losses
#[derive(Debug, Clone, Copy)]
pub struct BestCasePriority;

impl PriorityPolicy for BestCasePriority {
    fn prioritize(&self, orders: Vec<Order>, _bar: &Bar) -> Vec<Order> {
        let mut result = orders;

        // Prioritize take-profits over stop-losses (optimistic)
        result.sort_by_key(|order| {
            if self.is_take_profit(order) {
                0 // Targets first
            } else if self.is_stop_loss(order) {
                2 // Stops last
            } else {
                1 // Others middle
            }
        });

        result
    }

    fn name(&self) -> &str {
        "BestCasePriority"
    }
}

impl BestCasePriority {
    fn is_stop_loss(&self, order: &Order) -> bool {
        order.parent_id.is_some() && !self.is_take_profit(order)
    }

    fn is_take_profit(&self, order: &Order) -> bool {
        order.parent_id.is_some() && order.oco_sibling_id.is_some()
    }
}

/// PriceOrder priority: natural price-time sequence (FIFO)
#[derive(Debug, Clone, Copy)]
pub struct PriceOrderPriority;

impl PriorityPolicy for PriceOrderPriority {
    fn prioritize(&self, orders: Vec<Order>, _bar: &Bar) -> Vec<Order> {
        // Natural order (already sorted by path policy)
        // Could add time-based tiebreaking here
        orders
    }

    fn name(&self) -> &str {
        "PriceOrderPriority"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OrderId;
    use crate::orders::{OrderType, StopDirection};

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

    fn stop_order(id: u64) -> Order {
        Order::new(
            OrderId::from(id),
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 99.0,
            },
            100,
            0,
        )
    }

    #[test]
    fn test_worst_case_stops_first() {
        let policy = WorstCasePriority;
        let bar = test_bar();

        let mut stop = stop_order(1);
        stop.parent_id = Some(OrderId::from(100));

        let mut target = stop_order(2);
        target.parent_id = Some(OrderId::from(100));
        target.oco_sibling_id = Some(OrderId::from(1));

        let orders = vec![target.clone(), stop.clone()];
        let prioritized = policy.prioritize(orders, &bar);

        // Stop should be first (worst case)
        assert_eq!(prioritized[0].id, stop.id);
        assert_eq!(prioritized[1].id, target.id);
    }

    #[test]
    fn test_best_case_targets_first() {
        let policy = BestCasePriority;
        let bar = test_bar();

        let mut stop = stop_order(1);
        stop.parent_id = Some(OrderId::from(100));

        let mut target = stop_order(2);
        target.parent_id = Some(OrderId::from(100));
        target.oco_sibling_id = Some(OrderId::from(1));

        let orders = vec![stop.clone(), target.clone()];
        let prioritized = policy.prioritize(orders, &bar);

        // Target should be first (best case)
        assert_eq!(prioritized[0].id, target.id);
        assert_eq!(prioritized[1].id, stop.id);
    }

    #[test]
    fn test_price_order_preserves_sequence() {
        let policy = PriceOrderPriority;
        let bar = test_bar();

        let orders = vec![stop_order(1), stop_order(2), stop_order(3)];
        let prioritized = policy.prioritize(orders.clone(), &bar);

        // Should preserve original order
        assert_eq!(prioritized.len(), orders.len());
        for (i, order) in prioritized.iter().enumerate() {
            assert_eq!(order.id, orders[i].id);
        }
    }

    #[test]
    fn test_policy_names() {
        assert_eq!(WorstCasePriority.name(), "WorstCasePriority");
        assert_eq!(BestCasePriority.name(), "BestCasePriority");
        assert_eq!(PriceOrderPriority.name(), "PriceOrderPriority");
    }

    #[test]
    fn test_mixed_orders_worst_case() {
        let policy = WorstCasePriority;
        let bar = test_bar();

        let mut stop = stop_order(1);
        stop.parent_id = Some(OrderId::from(100));

        let mut target = stop_order(2);
        target.parent_id = Some(OrderId::from(100));
        target.oco_sibling_id = Some(OrderId::from(1));

        let regular = stop_order(3); // No parent/OCO

        let orders = vec![target.clone(), regular.clone(), stop.clone()];
        let prioritized = policy.prioritize(orders, &bar);

        // Order: stop, regular, target
        assert_eq!(prioritized[0].id, stop.id);
        assert_eq!(prioritized[1].id, regular.id);
        assert_eq!(prioritized[2].id, target.id);
    }
}
