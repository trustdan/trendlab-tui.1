//! Execution engine — computes fill prices and processes orders per bar phase.
//!
//! The execution engine is stateless: it carries only configuration parameters.
//! It borrows the order book and bar data, producing `Fill` records that the
//! caller applies to the portfolio.
//!
//! Three phase methods map to the event loop phases:
//! - `process_start_of_bar`: MOO and MarketImmediate fills
//! - `process_intrabar`: stop/limit triggers with path policy resolution
//! - `process_end_of_bar`: MOC fills

pub mod cost_model;
pub mod fill_price;
pub mod liquidity;
pub mod path_policy;
pub mod trigger;

pub use cost_model::CostModel;
pub use liquidity::{LiquidityPolicy, RemainderPolicy};

use crate::components::execution::{ExecutionPreset, GapPolicy, PathPolicy};
use crate::domain::instrument::Instrument;
use crate::domain::position::PositionSide;
use crate::domain::{Bar, Fill, FillPhase, OrderId, OrderStatus, OrderType};
use crate::engine::order_book::OrderBook;

use self::fill_price::compute_fill;
use self::trigger::{check_trigger, TriggerResult};

use std::collections::HashMap;

/// Configuration for the execution engine.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub cost_model: CostModel,
    pub path_policy: PathPolicy,
    pub gap_policy: GapPolicy,
    pub liquidity: Option<LiquidityPolicy>,
}

impl ExecutionConfig {
    pub fn from_preset(preset: ExecutionPreset) -> Self {
        Self {
            cost_model: CostModel::from_preset(preset),
            path_policy: preset.path_policy(),
            gap_policy: preset.gap_policy(),
            liquidity: None,
        }
    }

    pub fn frictionless() -> Self {
        Self::from_preset(ExecutionPreset::Frictionless)
    }
}

/// The execution engine: computes fill prices and manages order execution.
///
/// Stateless — carries only configuration. All mutable state lives in the
/// `OrderBook` and `Portfolio` (owned by `EngineState`).
pub struct ExecutionEngine {
    config: ExecutionConfig,
}

impl ExecutionEngine {
    pub fn new(config: ExecutionConfig) -> Self {
        Self { config }
    }

    pub fn from_preset(preset: ExecutionPreset) -> Self {
        Self::new(ExecutionConfig::from_preset(preset))
    }

    /// Phase 1: Start-of-bar.
    ///
    /// Fills MOO and MarketImmediate orders at the bar's open price.
    pub fn process_start_of_bar(
        &self,
        order_book: &mut OrderBook,
        bars: &HashMap<&str, &Bar>,
        instruments: &HashMap<String, Instrument>,
        bar_index: usize,
    ) -> Vec<Fill> {
        let mut fills = Vec::new();

        // Collect active MOO and Immediate orders
        let active: Vec<(OrderId, String)> = order_book
            .active_orders()
            .iter()
            .filter(|o| {
                matches!(
                    o.order_type,
                    OrderType::MarketOnOpen | OrderType::MarketImmediate
                )
            })
            .map(|o| (o.id, o.symbol.clone()))
            .collect();

        for (order_id, symbol) in active {
            let Some(bar) = bars.get(symbol.as_str()) else {
                continue;
            };
            if bar.is_void() {
                continue;
            }

            let order = match order_book.get(order_id) {
                Some(o) if o.is_active() => o,
                _ => continue,
            };

            let instrument = instruments
                .get(&symbol)
                .cloned()
                .unwrap_or_else(|| Instrument::us_equity(&symbol));

            let qty = self.effective_fill_qty(order.remaining_quantity(), bar.volume);
            if qty <= 0.0 {
                continue;
            }

            let computed = compute_fill(
                bar.open,
                order.side,
                qty,
                &instrument,
                &self.config.cost_model,
            );

            let fill = Fill {
                order_id,
                bar_index,
                date: bar.date,
                symbol: symbol.clone(),
                side: order.side,
                price: computed.price,
                quantity: qty,
                commission: computed.commission,
                slippage: computed.slippage,
                phase: FillPhase::StartOfBar,
            };

            // Record fill in order book (handles OCO, bracket activation)
            let _ = order_book.record_fill(order_id, qty, bar_index);
            fills.push(fill);
        }

        fills
    }

    /// Phase 2: Intrabar.
    ///
    /// Checks stop/limit triggers against bar high/low range.
    /// Resolves ambiguous bars via path policy.
    /// Applies gap rules for gap-through stops.
    pub fn process_intrabar(
        &self,
        order_book: &mut OrderBook,
        bars: &HashMap<&str, &Bar>,
        instruments: &HashMap<String, Instrument>,
        bar_index: usize,
        position_sides: &HashMap<String, PositionSide>,
    ) -> Vec<Fill> {
        let mut fills = Vec::new();

        // Get all symbols that have active orders
        let symbols_with_orders: Vec<String> = {
            let active = order_book.active_orders();
            let mut syms: Vec<String> = active.iter().map(|o| o.symbol.clone()).collect();
            syms.sort();
            syms.dedup();
            syms
        };

        for symbol in symbols_with_orders {
            let Some(bar) = bars.get(symbol.as_str()) else {
                continue;
            };
            if bar.is_void() {
                continue;
            }

            let instrument = instruments
                .get(&symbol)
                .cloned()
                .unwrap_or_else(|| Instrument::us_equity(&symbol));

            // Get active stop/limit orders for this symbol
            let active_orders: Vec<(OrderId, OrderType, crate::domain::instrument::OrderSide)> =
                order_book
                .active_orders_for_symbol(&symbol)
                .iter()
                .filter(|o| {
                    matches!(
                        o.order_type,
                        OrderType::StopMarket { .. }
                            | OrderType::Limit { .. }
                            | OrderType::StopLimit { .. }
                    )
                })
                // Skip bracket children activated this bar (same-bar entry+exit prevention)
                .filter(|o| o.activated_bar != Some(bar_index))
                .map(|o| (o.id, o.order_type.clone(), o.side))
                .collect();

            if active_orders.is_empty() {
                continue;
            }

            // Get evaluation order from path policy
            let order_refs: Vec<&crate::domain::Order> = active_orders
                .iter()
                .filter_map(|(id, _, _)| order_book.get(*id))
                .filter(|o| o.is_active())
                .collect();

            let position_side = position_sides.get(&symbol).copied();
            let eval_order = path_policy::order_evaluation_sequence(
                &order_refs,
                position_side,
                self.config.path_policy,
                bar,
            );

            // Evaluate orders in path-policy order
            for order_id in eval_order {
                // Re-fetch order (state may have changed from OCO cancellation)
                let order = match order_book.get(order_id) {
                    Some(o) if o.is_active() => o,
                    _ => continue, // cancelled by OCO
                };

                let result = check_trigger(order, bar, self.config.gap_policy);
                match result {
                    TriggerResult::Fill { fill_price, .. } => {
                        let qty = self.effective_fill_qty(order.remaining_quantity(), bar.volume);
                        if qty <= 0.0 {
                            continue;
                        }

                        let computed = compute_fill(
                            fill_price,
                            order.side,
                            qty,
                            &instrument,
                            &self.config.cost_model,
                        );

                        let fill = Fill {
                            order_id,
                            bar_index,
                            date: bar.date,
                            symbol: symbol.clone(),
                            side: order.side,
                            price: computed.price,
                            quantity: qty,
                            commission: computed.commission,
                            slippage: computed.slippage,
                            phase: FillPhase::Intrabar,
                        };

                        // For stops: trigger first, then fill
                        if matches!(order.order_type, OrderType::StopMarket { .. })
                            && order.status == OrderStatus::Pending
                        {
                            let _ = order_book.trigger(order_id, bar_index);
                        }

                        let _ = order_book.record_fill(order_id, qty, bar_index);
                        fills.push(fill);
                    }
                    TriggerResult::StopTriggeredLimitPending => {
                        // StopLimit: trigger the stop, leave limit pending
                        let _ = order_book.trigger(order_id, bar_index);
                    }
                    TriggerResult::NoTrigger => {}
                }
            }
        }

        fills
    }

    /// Phase 3: End-of-bar.
    ///
    /// Fills MOC orders at the bar's close price.
    pub fn process_end_of_bar(
        &self,
        order_book: &mut OrderBook,
        bars: &HashMap<&str, &Bar>,
        instruments: &HashMap<String, Instrument>,
        bar_index: usize,
    ) -> Vec<Fill> {
        let mut fills = Vec::new();

        // Collect active MOC orders
        let active: Vec<(OrderId, String)> = order_book
            .active_orders()
            .iter()
            .filter(|o| matches!(o.order_type, OrderType::MarketOnClose))
            .map(|o| (o.id, o.symbol.clone()))
            .collect();

        for (order_id, symbol) in active {
            let Some(bar) = bars.get(symbol.as_str()) else {
                continue;
            };
            if bar.is_void() {
                continue;
            }

            let order = match order_book.get(order_id) {
                Some(o) if o.is_active() => o,
                _ => continue,
            };

            let instrument = instruments
                .get(&symbol)
                .cloned()
                .unwrap_or_else(|| Instrument::us_equity(&symbol));

            let qty = self.effective_fill_qty(order.remaining_quantity(), bar.volume);
            if qty <= 0.0 {
                continue;
            }

            let computed = compute_fill(
                bar.close,
                order.side,
                qty,
                &instrument,
                &self.config.cost_model,
            );

            let fill = Fill {
                order_id,
                bar_index,
                date: bar.date,
                symbol: symbol.clone(),
                side: order.side,
                price: computed.price,
                quantity: qty,
                commission: computed.commission,
                slippage: computed.slippage,
                phase: FillPhase::EndOfBar,
            };

            let _ = order_book.record_fill(order_id, qty, bar_index);
            fills.push(fill);
        }

        fills
    }

    /// Apply liquidity constraint to desired quantity. Returns effective fill qty.
    fn effective_fill_qty(&self, desired_qty: f64, bar_volume: u64) -> f64 {
        match &self.config.liquidity {
            Some(policy) => {
                let (fill_qty, _remainder) = policy.constrain(desired_qty, bar_volume);
                fill_qty
            }
            None => desired_qty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::OcoGroupId;
    use crate::domain::instrument::OrderSide;
    use crate::domain::{Order, OrderId, OrderStatus, OrderType};
    use chrono::NaiveDate;

    fn bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            symbol: "SPY".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open,
            high,
            low,
            close,
            volume: 1_000_000,
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

    fn default_instruments() -> HashMap<String, Instrument> {
        let mut m = HashMap::new();
        m.insert("SPY".into(), Instrument::us_equity("SPY"));
        m
    }

    // ── Start-of-bar tests ──────────────────────────────────────────

    #[test]
    fn start_of_bar_fills_moo() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

        let b = bar(100.0, 105.0, 98.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);

        let fills = engine.process_start_of_bar(&mut book, &bars, &default_instruments(), 0);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 100.0);
        assert_eq!(fills[0].quantity, 100.0);
        assert_eq!(fills[0].phase, FillPhase::StartOfBar);
        assert_eq!(book.get(OrderId(1)).unwrap().status, OrderStatus::Filled);
    }

    #[test]
    fn start_of_bar_fills_immediate() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Buy, OrderType::MarketImmediate));

        let b = bar(100.0, 105.0, 98.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);

        let fills = engine.process_start_of_bar(&mut book, &bars, &default_instruments(), 0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 100.0);
    }

    #[test]
    fn start_of_bar_skips_void_bar() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

        let b = Bar {
            symbol: "SPY".into(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: f64::NAN,
            high: f64::NAN,
            low: f64::NAN,
            close: f64::NAN,
            volume: 0,
            adj_close: f64::NAN,
        };
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);

        let fills = engine.process_start_of_bar(&mut book, &bars, &default_instruments(), 0);
        assert!(fills.is_empty());
        assert!(book.get(OrderId(1)).unwrap().is_active()); // still pending
    }

    // ── Intrabar tests ──────────────────────────────────────────────

    #[test]
    fn intrabar_fills_stop_market() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();
        book.submit(make_order(
            1,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 98.0,
            },
        ));

        let b = bar(100.0, 105.0, 97.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);
        let positions = HashMap::new();

        let fills =
            engine.process_intrabar(&mut book, &bars, &default_instruments(), 0, &positions);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 98.0); // trigger price, no gap
        assert_eq!(fills[0].phase, FillPhase::Intrabar);
    }

    #[test]
    fn intrabar_gap_through_fills_at_open() {
        let config = ExecutionConfig {
            cost_model: CostModel::frictionless(),
            path_policy: PathPolicy::WorstCase,
            gap_policy: GapPolicy::FillAtOpen,
            liquidity: None,
        };
        let engine = ExecutionEngine::new(config);
        let mut book = OrderBook::new();
        book.submit(make_order(
            1,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 100.0,
            },
        ));

        // Gap down: open at 95, below trigger 100
        let b = bar(95.0, 97.0, 93.0, 96.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);
        let positions = HashMap::new();

        let fills =
            engine.process_intrabar(&mut book, &bars, &default_instruments(), 0, &positions);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 95.0); // filled at open, not trigger
    }

    #[test]
    fn intrabar_fills_limit_order() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();
        book.submit(make_order(
            1,
            OrderSide::Buy,
            OrderType::Limit { limit_price: 98.0 },
        ));

        let b = bar(100.0, 105.0, 97.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);
        let positions = HashMap::new();

        let fills =
            engine.process_intrabar(&mut book, &bars, &default_instruments(), 0, &positions);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 98.0);
    }

    #[test]
    fn intrabar_worst_case_stop_before_tp() {
        let engine = ExecutionEngine::new(ExecutionConfig {
            cost_model: CostModel::frictionless(),
            path_policy: PathPolicy::WorstCase,
            gap_policy: GapPolicy::FillAtOpen,
            liquidity: None,
        });
        let mut book = OrderBook::new();

        // OCO pair: stop and take-profit
        let mut stop = make_order(
            1,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 95.0,
            },
        );
        stop.oco_group_id = Some(OcoGroupId(10));
        let mut tp = make_order(2, OrderSide::Sell, OrderType::Limit { limit_price: 110.0 });
        tp.oco_group_id = Some(OcoGroupId(10));

        let oco = crate::domain::OcoGroup {
            id: OcoGroupId(10),
            order_ids: vec![OrderId(1), OrderId(2)],
        };
        book.submit(stop);
        book.submit(tp);
        book.register_oco_group(oco);

        // Ambiguous bar: both stop (95) and tp (110) reachable
        let b = bar(100.0, 112.0, 94.0, 105.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);
        let mut positions = HashMap::new();
        positions.insert("SPY".into(), PositionSide::Long);

        let fills =
            engine.process_intrabar(&mut book, &bars, &default_instruments(), 0, &positions);

        // WorstCase: stop should fill (adverse), tp should be cancelled (OCO)
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, OrderId(1)); // stop filled
        assert_eq!(fills[0].price, 95.0);

        // TP should be cancelled via OCO
        assert!(matches!(
            book.get(OrderId(2)).unwrap().status,
            OrderStatus::Cancelled { .. }
        ));
    }

    #[test]
    fn intrabar_same_bar_bracket_children_not_filled() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();

        // Entry that will fill, plus a stop-loss child
        let entry = make_order(1, OrderSide::Buy, OrderType::MarketOnOpen);
        let mut stop = make_order(
            2,
            OrderSide::Sell,
            OrderType::StopMarket {
                trigger_price: 95.0,
            },
        );
        stop.oco_group_id = Some(OcoGroupId(20));

        book.submit_bracket(entry, stop, None, OcoGroupId(20));

        // Fill entry in start-of-bar
        let b = bar(100.0, 105.0, 94.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);

        let _ = engine.process_start_of_bar(&mut book, &bars, &default_instruments(), 0);

        // Entry filled → child activated with activated_bar = 0
        assert!(book.get(OrderId(2)).unwrap().is_active());
        assert_eq!(book.get(OrderId(2)).unwrap().activated_bar, Some(0));

        // Now intrabar: stop at 95 is reachable (low=94), but it was activated this bar
        let positions = HashMap::new();
        let fills =
            engine.process_intrabar(&mut book, &bars, &default_instruments(), 0, &positions);

        // Stop should NOT fill on same bar
        assert!(fills.is_empty());
        assert!(book.get(OrderId(2)).unwrap().is_active()); // still pending
    }

    // ── End-of-bar tests ────────────────────────────────────────────

    #[test]
    fn end_of_bar_fills_moc() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Sell, OrderType::MarketOnClose));

        let b = bar(100.0, 105.0, 98.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);

        let fills = engine.process_end_of_bar(&mut book, &bars, &default_instruments(), 0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 103.0);
        assert_eq!(fills[0].phase, FillPhase::EndOfBar);
    }

    // ── Slippage/commission tests ───────────────────────────────────

    #[test]
    fn realistic_preset_applies_slippage() {
        let engine = ExecutionEngine::from_preset(ExecutionPreset::Realistic);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

        let b = bar(100.0, 105.0, 98.0, 103.0);
        let mut bars = HashMap::new();
        bars.insert("SPY", &b);

        let fills = engine.process_start_of_bar(&mut book, &bars, &default_instruments(), 0);
        assert_eq!(fills.len(), 1);
        // Buy with 5 bps slippage: 100 * 1.0005 = 100.05
        assert!(fills[0].price > 100.0);
        assert!(fills[0].slippage > 0.0);
        assert!(fills[0].commission > 0.0);
    }

    #[test]
    fn different_presets_different_fills() {
        let b = bar(100.0, 105.0, 98.0, 103.0);

        let mut results = Vec::new();
        for preset in [
            ExecutionPreset::Frictionless,
            ExecutionPreset::Optimistic,
            ExecutionPreset::Realistic,
            ExecutionPreset::Hostile,
        ] {
            let engine = ExecutionEngine::from_preset(preset);
            let mut book = OrderBook::new();
            book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

            let mut bars = HashMap::new();
            bars.insert("SPY", &b);

            let fills = engine.process_start_of_bar(&mut book, &bars, &default_instruments(), 0);
            results.push(fills[0].price);
        }

        // Frictionless < Optimistic < Realistic < Hostile (for buy orders)
        assert!(results[0] <= results[1]);
        assert!(results[1] <= results[2]);
        assert!(results[2] <= results[3]);
    }
}
