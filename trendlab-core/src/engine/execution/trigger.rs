//! Trigger checking — does a bar trigger a given order?
//!
//! Evaluates whether an order's trigger condition is met within a bar's
//! OHLC range. Computes the raw fill price before slippage/commission.
//! Handles gap-through fills per the configured GapPolicy.

use crate::components::execution::GapPolicy;
use crate::domain::instrument::OrderSide;
use crate::domain::{Bar, Order, OrderStatus, OrderType};

/// Result of checking whether an order triggers on a bar.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerResult {
    /// Order does not trigger on this bar.
    NoTrigger,
    /// Order triggers and fills at the computed price.
    Fill { fill_price: f64, gap_through: bool },
    /// StopLimit: stop triggers but limit is not reached on this bar.
    /// Order transitions to Triggered but does not fill yet.
    StopTriggeredLimitPending,
}

/// Check whether an order triggers on a given bar and compute its raw fill price.
///
/// Does NOT apply slippage or commission — that happens in the cost model.
/// Does NOT check `activated_bar` — the caller must skip same-bar bracket children.
pub fn check_trigger(order: &Order, bar: &Bar, gap_policy: GapPolicy) -> TriggerResult {
    if bar.is_void() {
        return TriggerResult::NoTrigger;
    }

    match &order.order_type {
        OrderType::MarketOnOpen => TriggerResult::Fill {
            fill_price: bar.open,
            gap_through: false,
        },
        OrderType::MarketOnClose => TriggerResult::Fill {
            fill_price: bar.close,
            gap_through: false,
        },
        OrderType::MarketImmediate => TriggerResult::Fill {
            fill_price: bar.open,
            gap_through: false,
        },
        OrderType::StopMarket { trigger_price } => {
            check_stop_market(order.side, *trigger_price, bar, gap_policy)
        }
        OrderType::Limit { limit_price } => check_limit(order.side, *limit_price, bar),
        OrderType::StopLimit {
            trigger_price,
            limit_price,
        } => check_stop_limit(
            order.side,
            &order.status,
            *trigger_price,
            *limit_price,
            bar,
            gap_policy,
        ),
    }
}

/// Check a stop-market order trigger.
///
/// Sell stop: triggers if bar.low <= trigger. Gap-through if open <= trigger.
/// Buy stop: triggers if bar.high >= trigger. Gap-through if open >= trigger.
fn check_stop_market(
    side: OrderSide,
    trigger: f64,
    bar: &Bar,
    gap_policy: GapPolicy,
) -> TriggerResult {
    match side {
        OrderSide::Sell => {
            // Sell stop triggers when price falls to or below trigger
            if bar.low <= trigger {
                let gap_through = bar.open <= trigger;
                let fill_price = if gap_through {
                    resolve_gap_sell(bar.open, trigger, gap_policy)
                } else {
                    trigger
                };
                TriggerResult::Fill {
                    fill_price,
                    gap_through,
                }
            } else {
                TriggerResult::NoTrigger
            }
        }
        OrderSide::Buy => {
            // Buy stop triggers when price rises to or above trigger
            if bar.high >= trigger {
                let gap_through = bar.open >= trigger;
                let fill_price = if gap_through {
                    resolve_gap_buy(bar.open, trigger, gap_policy)
                } else {
                    trigger
                };
                TriggerResult::Fill {
                    fill_price,
                    gap_through,
                }
            } else {
                TriggerResult::NoTrigger
            }
        }
    }
}

/// Check a limit order trigger.
///
/// Buy limit: triggers if bar.low <= limit. Fills at limit (or better at open if gap-through).
/// Sell limit: triggers if bar.high >= limit. Fills at limit (or better at open if gap-through).
fn check_limit(side: OrderSide, limit: f64, bar: &Bar) -> TriggerResult {
    match side {
        OrderSide::Buy => {
            if bar.low <= limit {
                // Favorable gap: open is below limit → fill at the better price
                let fill_price = if bar.open <= limit { bar.open } else { limit };
                TriggerResult::Fill {
                    fill_price,
                    gap_through: bar.open <= limit,
                }
            } else {
                TriggerResult::NoTrigger
            }
        }
        OrderSide::Sell => {
            if bar.high >= limit {
                // Favorable gap: open is above limit → fill at the better price
                let fill_price = if bar.open >= limit { bar.open } else { limit };
                TriggerResult::Fill {
                    fill_price,
                    gap_through: bar.open >= limit,
                }
            } else {
                TriggerResult::NoTrigger
            }
        }
    }
}

/// Check a stop-limit order trigger.
///
/// Two-stage: the stop triggers first (Pending → Triggered), then the limit
/// condition must be met. Can trigger and fill on the same bar if the range allows.
fn check_stop_limit(
    side: OrderSide,
    status: &OrderStatus,
    trigger: f64,
    limit: f64,
    bar: &Bar,
    _gap_policy: GapPolicy,
) -> TriggerResult {
    match status {
        OrderStatus::Pending => {
            // Stage 1: check if stop triggers
            let triggers = match side {
                OrderSide::Buy => bar.high >= trigger,
                OrderSide::Sell => bar.low <= trigger,
            };
            if !triggers {
                return TriggerResult::NoTrigger;
            }
            // Stop triggered — now check if limit can also fill on this bar
            let limit_result = check_limit(side, limit, bar);
            match limit_result {
                TriggerResult::Fill { .. } => limit_result, // same-bar trigger+fill
                _ => TriggerResult::StopTriggeredLimitPending,
            }
        }
        OrderStatus::Triggered => {
            // Stage 2: already triggered, just check limit
            check_limit(side, limit, bar)
        }
        _ => TriggerResult::NoTrigger,
    }
}

/// Resolve gap-through fill price for a sell stop.
///
/// When price gaps down through a sell stop (open < trigger):
/// - FillAtOpen: fill at open (worse for seller, realistic default)
/// - FillAtTrigger: fill at trigger (optimistic)
/// - FillAtWorst: fill at worse of the two (min for seller)
fn resolve_gap_sell(open: f64, trigger: f64, policy: GapPolicy) -> f64 {
    match policy {
        GapPolicy::FillAtOpen => open,
        GapPolicy::FillAtTrigger => trigger,
        GapPolicy::FillAtWorst => open.min(trigger), // lower is worse for seller
    }
}

/// Resolve gap-through fill price for a buy stop.
///
/// When price gaps up through a buy stop (open > trigger):
/// - FillAtOpen: fill at open (worse for buyer, realistic default)
/// - FillAtTrigger: fill at trigger (optimistic)
/// - FillAtWorst: fill at worse of the two (max for buyer)
fn resolve_gap_buy(open: f64, trigger: f64, policy: GapPolicy) -> f64 {
    match policy {
        GapPolicy::FillAtOpen => open,
        GapPolicy::FillAtTrigger => trigger,
        GapPolicy::FillAtWorst => open.max(trigger), // higher is worse for buyer
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

    fn void_bar() -> Bar {
        Bar {
            symbol: "SPY".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: f64::NAN,
            high: f64::NAN,
            low: f64::NAN,
            close: f64::NAN,
            volume: 0,
            adj_close: f64::NAN,
        }
    }

    fn make_order(side: OrderSide, order_type: OrderType) -> Order {
        Order {
            id: OrderId(1),
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

    fn make_triggered_order(side: OrderSide, order_type: OrderType) -> Order {
        let mut o = make_order(side, order_type);
        o.status = OrderStatus::Triggered;
        o
    }

    // ── MarketOnOpen ─────────────────────────────────────────────────

    #[test]
    fn moo_fills_at_open() {
        let order = make_order(OrderSide::Buy, OrderType::MarketOnOpen);
        let b = bar(100.0, 105.0, 98.0, 103.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 100.0,
                gap_through: false
            }
        );
    }

    // ── MarketOnClose ────────────────────────────────────────────────

    #[test]
    fn moc_fills_at_close() {
        let order = make_order(OrderSide::Sell, OrderType::MarketOnClose);
        let b = bar(100.0, 105.0, 98.0, 103.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 103.0,
                gap_through: false
            }
        );
    }

    // ── StopMarket sell ──────────────────────────────────────────────

    #[test]
    fn sell_stop_triggers_when_low_reaches_trigger() {
        let order = make_order(
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 98.0,
            },
        );
        let b = bar(100.0, 105.0, 97.0, 99.0); // low 97 <= trigger 98
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 98.0,
                gap_through: false
            }
        );
    }

    #[test]
    fn sell_stop_does_not_trigger_above() {
        let order = make_order(
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 95.0,
            },
        );
        let b = bar(100.0, 105.0, 98.0, 103.0); // low 98 > trigger 95
        assert_eq!(
            check_trigger(&order, &b, GapPolicy::FillAtOpen),
            TriggerResult::NoTrigger
        );
    }

    #[test]
    fn sell_stop_gap_through_fill_at_open() {
        let order = make_order(
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 100.0,
            },
        );
        // Gap down: open at 95, below trigger of 100
        let b = bar(95.0, 97.0, 93.0, 96.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 95.0,
                gap_through: true
            }
        );
    }

    #[test]
    fn sell_stop_gap_through_fill_at_trigger() {
        let order = make_order(
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 100.0,
            },
        );
        let b = bar(95.0, 97.0, 93.0, 96.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtTrigger);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 100.0,
                gap_through: true
            }
        );
    }

    #[test]
    fn sell_stop_gap_through_fill_at_worst() {
        let order = make_order(
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 100.0,
            },
        );
        let b = bar(95.0, 97.0, 93.0, 96.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtWorst);
        // For seller, open (95) is worse than trigger (100), so fill at 95
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 95.0,
                gap_through: true
            }
        );
    }

    // ── StopMarket buy ───────────────────────────────────────────────

    #[test]
    fn buy_stop_triggers_when_high_reaches_trigger() {
        let order = make_order(
            OrderSide::Buy,
            OrderType::StopMarket {
                trigger_price: 105.0,
            },
        );
        let b = bar(100.0, 106.0, 98.0, 103.0); // high 106 >= trigger 105
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 105.0,
                gap_through: false
            }
        );
    }

    #[test]
    fn buy_stop_gap_through_fill_at_open() {
        let order = make_order(
            OrderSide::Buy,
            OrderType::StopMarket {
                trigger_price: 100.0,
            },
        );
        // Gap up: open at 105, above trigger of 100
        let b = bar(105.0, 108.0, 103.0, 107.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        // Buyer pays 105 (open) instead of 100 (trigger) — worse
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 105.0,
                gap_through: true
            }
        );
    }

    #[test]
    fn buy_stop_gap_through_fill_at_worst() {
        let order = make_order(
            OrderSide::Buy,
            OrderType::StopMarket {
                trigger_price: 100.0,
            },
        );
        let b = bar(105.0, 108.0, 103.0, 107.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtWorst);
        // For buyer, open (105) is worse than trigger (100), so fill at 105
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 105.0,
                gap_through: true
            }
        );
    }

    // ── Limit buy ────────────────────────────────────────────────────

    #[test]
    fn buy_limit_triggers_when_low_reaches_limit() {
        let order = make_order(OrderSide::Buy, OrderType::Limit { limit_price: 98.0 });
        let b = bar(100.0, 105.0, 97.0, 103.0); // low 97 <= limit 98
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 98.0,
                gap_through: false
            }
        );
    }

    #[test]
    fn buy_limit_gap_favorable_fills_at_open() {
        let order = make_order(OrderSide::Buy, OrderType::Limit { limit_price: 100.0 });
        // Gap down: open at 95, below limit of 100 → buyer gets better price
        let b = bar(95.0, 97.0, 93.0, 96.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 95.0,
                gap_through: true
            }
        );
    }

    #[test]
    fn buy_limit_not_triggered() {
        let order = make_order(OrderSide::Buy, OrderType::Limit { limit_price: 95.0 });
        let b = bar(100.0, 105.0, 98.0, 103.0); // low 98 > limit 95
        assert_eq!(
            check_trigger(&order, &b, GapPolicy::FillAtOpen),
            TriggerResult::NoTrigger
        );
    }

    // ── Limit sell ───────────────────────────────────────────────────

    #[test]
    fn sell_limit_triggers_when_high_reaches_limit() {
        let order = make_order(OrderSide::Sell, OrderType::Limit { limit_price: 108.0 });
        let b = bar(100.0, 110.0, 98.0, 105.0); // high 110 >= limit 108
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 108.0,
                gap_through: false
            }
        );
    }

    #[test]
    fn sell_limit_gap_favorable_fills_at_open() {
        let order = make_order(OrderSide::Sell, OrderType::Limit { limit_price: 100.0 });
        // Gap up: open at 105, above limit of 100 → seller gets better price
        let b = bar(105.0, 108.0, 103.0, 107.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 105.0,
                gap_through: true
            }
        );
    }

    // ── StopLimit ────────────────────────────────────────────────────

    #[test]
    fn stop_limit_pending_trigger_and_fill_same_bar() {
        // Buy stop-limit: trigger at 105, limit at 106
        let order = make_order(
            OrderSide::Buy,
            OrderType::StopLimit {
                trigger_price: 105.0,
                limit_price: 106.0,
            },
        );
        // Bar where high reaches trigger AND low reaches limit
        let b = bar(100.0, 108.0, 98.0, 103.0);
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert!(matches!(result, TriggerResult::Fill { .. }));
    }

    #[test]
    fn stop_limit_pending_trigger_but_limit_not_reached() {
        // Buy stop-limit: trigger at 105, limit at 95
        let order = make_order(
            OrderSide::Buy,
            OrderType::StopLimit {
                trigger_price: 105.0,
                limit_price: 95.0,
            },
        );
        // High reaches trigger but low doesn't reach limit
        let b = bar(100.0, 108.0, 98.0, 103.0); // low 98 > limit 95
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(result, TriggerResult::StopTriggeredLimitPending);
    }

    #[test]
    fn stop_limit_already_triggered_fills_at_limit() {
        let order = make_triggered_order(
            OrderSide::Buy,
            OrderType::StopLimit {
                trigger_price: 105.0,
                limit_price: 98.0,
            },
        );
        let b = bar(100.0, 105.0, 97.0, 103.0); // low 97 <= limit 98
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 98.0,
                gap_through: false
            }
        );
    }

    #[test]
    fn stop_limit_not_triggered_yet() {
        let order = make_order(
            OrderSide::Buy,
            OrderType::StopLimit {
                trigger_price: 110.0,
                limit_price: 108.0,
            },
        );
        let b = bar(100.0, 105.0, 98.0, 103.0); // high 105 < trigger 110
        assert_eq!(
            check_trigger(&order, &b, GapPolicy::FillAtOpen),
            TriggerResult::NoTrigger
        );
    }

    // ── Void bar ─────────────────────────────────────────────────────

    #[test]
    fn void_bar_never_triggers() {
        let order = make_order(OrderSide::Buy, OrderType::MarketOnOpen);
        let b = void_bar();
        assert_eq!(
            check_trigger(&order, &b, GapPolicy::FillAtOpen),
            TriggerResult::NoTrigger
        );
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn sell_stop_triggers_at_exact_level() {
        let order = make_order(
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 98.0,
            },
        );
        let b = bar(100.0, 105.0, 98.0, 103.0); // low == trigger exactly
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 98.0,
                gap_through: false
            }
        );
    }

    #[test]
    fn buy_limit_triggers_at_exact_level() {
        let order = make_order(OrderSide::Buy, OrderType::Limit { limit_price: 98.0 });
        let b = bar(100.0, 105.0, 98.0, 103.0); // low == limit exactly
        let result = check_trigger(&order, &b, GapPolicy::FillAtOpen);
        assert_eq!(
            result,
            TriggerResult::Fill {
                fill_price: 98.0,
                gap_through: false
            }
        );
    }
}
