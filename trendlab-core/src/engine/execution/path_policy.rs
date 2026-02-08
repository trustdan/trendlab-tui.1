//! Path policy — resolve ambiguous bars where multiple orders could trigger.
//!
//! When a bar's high-low range encompasses both a stop-loss and a take-profit,
//! the path policy determines which order is evaluated first.

use crate::components::execution::PathPolicy;
use crate::domain::instrument::OrderSide;
use crate::domain::position::PositionSide;
use crate::domain::{Bar, Order, OrderId, OrderType};

/// Determine the evaluation order for active orders on a symbol.
///
/// Returns ordered `OrderId`s. The first order in the sequence is evaluated
/// first; if it fills, OCO siblings are cancelled before evaluating later orders.
pub fn order_evaluation_sequence(
    orders: &[&Order],
    position_side: Option<PositionSide>,
    policy: PathPolicy,
    bar: &Bar,
) -> Vec<OrderId> {
    if orders.len() <= 1 {
        return orders.iter().map(|o| o.id).collect();
    }

    match policy {
        PathPolicy::WorstCase => worst_case_order(orders, position_side),
        PathPolicy::BestCase => best_case_order(orders, position_side),
        PathPolicy::Deterministic => deterministic_order(orders, bar),
    }
}

/// WorstCase: evaluate adverse orders first.
///
/// For a long position: stop-losses (sells at loss) before take-profits (sells at gain).
/// For a short position: buy-stops (buys at loss) before take-profit limits.
/// When no position: evaluate in a way that produces the worst entry price.
fn worst_case_order(orders: &[&Order], position_side: Option<PositionSide>) -> Vec<OrderId> {
    let mut adverse = Vec::new();
    let mut favorable = Vec::new();

    for order in orders {
        if is_adverse(order, position_side) {
            adverse.push(order.id);
        } else {
            favorable.push(order.id);
        }
    }

    adverse.extend(favorable);
    adverse
}

/// BestCase: evaluate favorable orders first.
fn best_case_order(orders: &[&Order], position_side: Option<PositionSide>) -> Vec<OrderId> {
    let mut favorable = Vec::new();
    let mut adverse = Vec::new();

    for order in orders {
        if is_adverse(order, position_side) {
            adverse.push(order.id);
        } else {
            favorable.push(order.id);
        }
    }

    favorable.extend(adverse);
    favorable
}

/// Deterministic: infer price path from OHLC, evaluate in path order.
///
/// Heuristic: if |open - high| <= |open - low|, the price went to the high
/// first (path: Open → High → Low → Close). Otherwise Open → Low → High → Close.
///
/// Orders are sorted by their trigger prices relative to the inferred path.
fn deterministic_order(orders: &[&Order], bar: &Bar) -> Vec<OrderId> {
    let high_first = (bar.open - bar.high).abs() <= (bar.open - bar.low).abs();

    let mut with_prices: Vec<(OrderId, f64)> = orders
        .iter()
        .map(|o| {
            let price = trigger_price_of(o).unwrap_or(bar.open);
            (o.id, price)
        })
        .collect();

    if high_first {
        // Path: Open → High → Low → Close
        // Orders near high trigger first, then orders near low
        // Sort by distance from open in the upward direction first
        with_prices.sort_by(|a, b| {
            let a_up = a.1 >= bar.open;
            let b_up = b.1 >= bar.open;
            match (a_up, b_up) {
                (true, false) => std::cmp::Ordering::Less, // upward orders first
                (false, true) => std::cmp::Ordering::Greater, // downward orders second
                (true, true) => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal), // lower trigger first (reached first on way up)
                (false, false) => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal), // higher trigger first (reached first on way down)
            }
        });
    } else {
        // Path: Open → Low → High → Close
        // Orders near low trigger first, then orders near high
        with_prices.sort_by(|a, b| {
            let a_down = a.1 <= bar.open;
            let b_down = b.1 <= bar.open;
            match (a_down, b_down) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (true, true) => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal), // higher trigger first (reached first on way down)
                (false, false) => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal), // lower trigger first (reached first on way up)
            }
        });
    }

    with_prices.into_iter().map(|(id, _)| id).collect()
}

/// Classify an order as adverse (loss-producing) based on position side.
///
/// - Long position + Sell stop/market below reference = adverse (stop-loss)
/// - Short position + Buy stop/market above reference = adverse
/// - No position → entry orders: stops are adverse (worse entry on gap), limits are favorable
fn is_adverse(order: &Order, position_side: Option<PositionSide>) -> bool {
    match position_side {
        Some(PositionSide::Long) => {
            // For a long position, sell stops are adverse
            matches!(
                (&order.side, &order.order_type),
                (OrderSide::Sell, OrderType::StopMarket { .. })
            )
        }
        Some(PositionSide::Short) => {
            // For a short position, buy stops are adverse
            matches!(
                (&order.side, &order.order_type),
                (OrderSide::Buy, OrderType::StopMarket { .. })
            )
        }
        Some(PositionSide::Flat) | None => {
            // No position → classify stops as adverse (worse entry on gap)
            matches!(
                &order.order_type,
                OrderType::StopMarket { .. } | OrderType::StopLimit { .. }
            )
        }
    }
}

/// Extract the trigger/limit price from an order type (for path ordering).
fn trigger_price_of(order: &Order) -> Option<f64> {
    match &order.order_type {
        OrderType::StopMarket { trigger_price } => Some(*trigger_price),
        OrderType::Limit { limit_price } => Some(*limit_price),
        OrderType::StopLimit { trigger_price, .. } => Some(*trigger_price),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderId, OrderStatus};
    use chrono::NaiveDate;

    fn bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            symbol: "SPY".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open,
            high,
            low,
            close,
            volume: 10_000,
            adj_close: close,
        }
    }

    fn make_order(id: u64, side: OrderSide, order_type: OrderType) -> Order {
        Order {
            id: OrderId(id),
            symbol: "SPY".into(),
            side,
            order_type,
            quantity: 100.0,
            filled_quantity: 0.0,
            status: OrderStatus::Pending,
            created_bar: 0,
            parent_id: None,
            oco_group_id: None,
            activated_bar: None,
        }
    }

    #[test]
    fn worst_case_long_position_stop_first() {
        let stop = make_order(
            1,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 95.0,
            },
        );
        let tp = make_order(2, OrderSide::Sell, OrderType::Limit { limit_price: 110.0 });
        let orders: Vec<&Order> = vec![&tp, &stop]; // intentionally reversed

        let b = bar(100.0, 112.0, 94.0, 105.0);
        let seq =
            order_evaluation_sequence(&orders, Some(PositionSide::Long), PathPolicy::WorstCase, &b);

        // Stop-loss should come first (adverse)
        assert_eq!(seq[0], OrderId(1));
        assert_eq!(seq[1], OrderId(2));
    }

    #[test]
    fn best_case_long_position_tp_first() {
        let stop = make_order(
            1,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 95.0,
            },
        );
        let tp = make_order(2, OrderSide::Sell, OrderType::Limit { limit_price: 110.0 });
        let orders: Vec<&Order> = vec![&stop, &tp];

        let b = bar(100.0, 112.0, 94.0, 105.0);
        let seq =
            order_evaluation_sequence(&orders, Some(PositionSide::Long), PathPolicy::BestCase, &b);

        // Take-profit should come first (favorable)
        assert_eq!(seq[0], OrderId(2));
        assert_eq!(seq[1], OrderId(1));
    }

    #[test]
    fn deterministic_high_first_path() {
        // Bar where open is closer to high → Open→High→Low→Close
        let b = bar(100.0, 103.0, 92.0, 98.0);
        // |open - high| = 3, |open - low| = 8 → high_first

        let stop = make_order(
            1,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 95.0,
            },
        );
        let tp = make_order(2, OrderSide::Sell, OrderType::Limit { limit_price: 102.0 });
        let orders: Vec<&Order> = vec![&stop, &tp];

        let seq = order_evaluation_sequence(
            &orders,
            Some(PositionSide::Long),
            PathPolicy::Deterministic,
            &b,
        );

        // Path goes up first (high=103), so limit at 102 is reached before stop at 95
        assert_eq!(seq[0], OrderId(2)); // limit at 102 (above open, reached first on way up)
        assert_eq!(seq[1], OrderId(1)); // stop at 95 (below open, reached second on way down)
    }

    #[test]
    fn single_order_returns_itself() {
        let order = make_order(1, OrderSide::Buy, OrderType::MarketOnOpen);
        let orders: Vec<&Order> = vec![&order];
        let b = bar(100.0, 105.0, 98.0, 103.0);
        let seq = order_evaluation_sequence(&orders, None, PathPolicy::WorstCase, &b);
        assert_eq!(seq, vec![OrderId(1)]);
    }

    #[test]
    fn empty_orders_returns_empty() {
        let orders: Vec<&Order> = vec![];
        let b = bar(100.0, 105.0, 98.0, 103.0);
        let seq = order_evaluation_sequence(&orders, None, PathPolicy::WorstCase, &b);
        assert!(seq.is_empty());
    }
}
