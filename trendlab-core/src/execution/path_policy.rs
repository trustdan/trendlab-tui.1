//! Path policies: resolve intrabar ambiguity
//!
//! When multiple orders could trigger in the same bar, we need to determine
//! the sequence. This is critical for realistic execution simulation.

use crate::domain::Bar;
use crate::orders::Order;

/// Path policy: determines order sequence within a bar
pub trait PathPolicy: Send + Sync {
    /// Return orders in trigger sequence for this bar
    fn order_sequence(&self, orders: &[Order], bar: &Bar) -> Vec<Order>;

    /// Name of this policy (for logging/debugging)
    fn name(&self) -> &str;
}

/// Deterministic: OHLC order (O → L → H → C)
#[derive(Debug, Clone, Copy)]
pub struct Deterministic;

impl PathPolicy for Deterministic {
    fn order_sequence(&self, orders: &[Order], bar: &Bar) -> Vec<Order> {
        // Natural OHLC sequence: Open → Low → High → Close
        let mut result = orders.to_vec();

        // Sort by trigger price relative to OHLC path
        result.sort_by(|a, b| {
            let (a_phase, a_price) = self.ohlc_index(a, bar);
            let (b_phase, b_price) = self.ohlc_index(b, bar);
            // First sort by phase, then by price within phase
            match a_phase.cmp(&b_phase) {
                std::cmp::Ordering::Equal => {
                    // Within same phase, sort by price
                    a_price.partial_cmp(&b_price).unwrap_or(std::cmp::Ordering::Equal)
                }
                other => other,
            }
        });

        result
    }

    fn name(&self) -> &str {
        "Deterministic"
    }
}

impl Deterministic {
    /// Map order trigger to OHLC path index
    ///
    /// OHLC sequence: Open → Low → High → Close
    /// Phase 0: At open
    /// Phase 1: Moving from Open to Low
    /// Phase 2: Moving from Low to High
    /// Phase 3: Moving from High to Close
    fn ohlc_index(&self, order: &Order, bar: &Bar) -> (usize, f64) {
        if let Some(trigger) = order.order_type.trigger_price() {
            // Check if trigger is reached in each phase
            if trigger >= bar.low && trigger <= bar.open {
                // Hit during move from Open down to Low (phase 1)
                return (1, trigger);
            } else if trigger >= bar.low && trigger <= bar.high {
                // Hit during move from Low up to High (phase 2)
                return (2, trigger);
            } else if trigger >= bar.close && trigger <= bar.high {
                // Hit during move from High to Close (phase 3)
                return (3, trigger);
            }
            // Otherwise doesn't trigger in this bar
            (99, trigger)
        } else {
            // Market orders execute at open
            (0, bar.open)
        }
    }
}

/// WorstCase: adversarial ordering for exits (stop-loss before take-profit)
#[derive(Debug, Clone, Copy)]
pub struct WorstCase;

impl PathPolicy for WorstCase {
    fn order_sequence(&self, orders: &[Order], _bar: &Bar) -> Vec<Order> {
        let mut result = orders.to_vec();

        // Adversarial: stop-losses fill before take-profits
        // This is the most conservative assumption
        result.sort_by_key(|order| {
            if self.is_take_profit(order) {
                2 // Take-profits last
            } else if self.is_stop_loss(order) {
                0 // Stop-losses first
            } else {
                1 // Other orders in middle
            }
        });

        result
    }

    fn name(&self) -> &str {
        "WorstCase"
    }
}

impl WorstCase {
    fn is_stop_loss(&self, order: &Order) -> bool {
        // Heuristic: parent order suggests protective stop
        order.parent_id.is_some()
    }

    fn is_take_profit(&self, order: &Order) -> bool {
        // Heuristic: OCO sibling suggests target
        order.oco_sibling_id.is_some() && order.parent_id.is_some()
    }
}

/// BestCase: optimistic ordering (take-profit before stop-loss)
#[derive(Debug, Clone, Copy)]
pub struct BestCase;

impl PathPolicy for BestCase {
    fn order_sequence(&self, orders: &[Order], _bar: &Bar) -> Vec<Order> {
        let mut result = orders.to_vec();

        // Optimistic: take-profits fill before stop-losses
        result.sort_by_key(|order| {
            if self.is_take_profit(order) {
                0 // Take-profits first
            } else if self.is_stop_loss(order) {
                2 // Stop-losses last
            } else {
                1 // Other orders in middle
            }
        });

        result
    }

    fn name(&self) -> &str {
        "BestCase"
    }
}

impl BestCase {
    fn is_stop_loss(&self, order: &Order) -> bool {
        order.parent_id.is_some()
    }

    fn is_take_profit(&self, order: &Order) -> bool {
        order.oco_sibling_id.is_some() && order.parent_id.is_some()
    }
}

/// PriceOrder: natural price sequence based on OHLC
#[derive(Debug, Clone, Copy)]
pub struct PriceOrder;

impl PathPolicy for PriceOrder {
    fn order_sequence(&self, orders: &[Order], bar: &Bar) -> Vec<Order> {
        let mut result = orders.to_vec();

        // Sort by trigger price in natural price progression
        result.sort_by(|a, b| {
            let a_price = a.order_type.trigger_price().unwrap_or(bar.open);
            let b_price = b.order_type.trigger_price().unwrap_or(bar.open);
            a_price.partial_cmp(&b_price).unwrap()
        });

        result
    }

    fn name(&self) -> &str {
        "PriceOrder"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OrderId;
    use crate::orders::{Order, OrderType, StopDirection};

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

    fn stop_order(id: u64, trigger: f64) -> Order {
        Order::new(
            OrderId::from(id),
            "SPY".into(),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: trigger,
            },
            100,
            0,
        )
    }

    #[test]
    fn test_deterministic_ohlc_sequence() {
        let policy = Deterministic;
        let bar = test_bar();

        let orders = vec![
            stop_order(1, 101.5), // Would hit on high
            stop_order(2, 98.5),  // Would hit on low
            stop_order(3, 100.5), // Would hit early
        ];

        let sequence = policy.order_sequence(&orders, &bar);

        // Order should be: low (98.5), then middle (100.5), then high (101.5)
        assert_eq!(sequence[0].id, OrderId::from(2));
        assert_eq!(sequence[2].id, OrderId::from(1));
    }

    #[test]
    fn test_worst_case_stops_first() {
        let policy = WorstCase;
        let bar = test_bar();

        let mut stop = stop_order(1, 99.0);
        stop.parent_id = Some(OrderId::from(100)); // Mark as stop-loss

        let mut target = stop_order(2, 101.5);
        target.parent_id = Some(OrderId::from(100));
        target.oco_sibling_id = Some(OrderId::from(1)); // Mark as take-profit

        let orders = vec![target.clone(), stop.clone()];
        let sequence = policy.order_sequence(&orders, &bar);

        // Stop-loss should be first (worst case)
        assert_eq!(sequence[0].id, stop.id);
        assert_eq!(sequence[1].id, target.id);
    }

    #[test]
    fn test_best_case_targets_first() {
        let policy = BestCase;
        let bar = test_bar();

        let mut stop = stop_order(1, 99.0);
        stop.parent_id = Some(OrderId::from(100));

        let mut target = stop_order(2, 101.5);
        target.parent_id = Some(OrderId::from(100));
        target.oco_sibling_id = Some(OrderId::from(1));

        let orders = vec![stop.clone(), target.clone()];
        let sequence = policy.order_sequence(&orders, &bar);

        // Take-profit should be first (best case)
        assert_eq!(sequence[0].id, target.id);
        assert_eq!(sequence[1].id, stop.id);
    }

    #[test]
    fn test_price_order_natural_progression() {
        let policy = PriceOrder;
        let bar = test_bar();

        let orders = vec![
            stop_order(1, 101.5),
            stop_order(2, 98.5),
            stop_order(3, 100.0),
        ];

        let sequence = policy.order_sequence(&orders, &bar);

        // Should be sorted by trigger price: 98.5, 100.0, 101.5
        assert_eq!(sequence[0].id, OrderId::from(2));
        assert_eq!(sequence[1].id, OrderId::from(3));
        assert_eq!(sequence[2].id, OrderId::from(1));
    }

    #[test]
    fn test_policy_names() {
        assert_eq!(Deterministic.name(), "Deterministic");
        assert_eq!(WorstCase.name(), "WorstCase");
        assert_eq!(BestCase.name(), "BestCase");
        assert_eq!(PriceOrder.name(), "PriceOrder");
    }
}
