//! Integration tests for the execution engine.
//!
//! These tests exercise the full pipeline: order submission → execution engine
//! processing → portfolio update → equity verification. They cover gap fills,
//! ambiguous bars, preset comparison, directional slippage, same-bar
//! entry+exit prevention, and liquidity constraints.

use std::collections::HashMap;

use trendlab_core::domain::fill::FillPhase;
use trendlab_core::domain::ids::{OcoGroupId, OrderId};
use trendlab_core::domain::instrument::{Instrument, OrderSide};
use trendlab_core::domain::order::{Order, OrderStatus, OrderType};
use trendlab_core::domain::position::PositionSide;
use trendlab_core::domain::{Bar, Portfolio};
use trendlab_core::engine::execution::{
    CostModel, ExecutionConfig, ExecutionEngine, LiquidityPolicy, RemainderPolicy,
};
use trendlab_core::engine::order_book::OrderBook;
use trendlab_core::engine::portfolio_update::apply_fills;

use trendlab_core::components::execution::{ExecutionPreset, GapPolicy, PathPolicy};

use chrono::NaiveDate;

// ─── Helpers ──────────────────────────────────────────────────────────

fn bar(symbol: &str, open: f64, high: f64, low: f64, close: f64) -> Bar {
    Bar {
        symbol: symbol.into(),
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

fn instruments() -> HashMap<String, Instrument> {
    let mut m = HashMap::new();
    m.insert("SPY".into(), Instrument::us_equity("SPY"));
    m
}

fn bars_map(b: &Bar) -> HashMap<&str, &Bar> {
    let mut m = HashMap::new();
    m.insert("SPY", b);
    m
}

// ─── Gap-through fill tests ──────────────────────────────────────────

#[test]
fn gap_through_sell_stop_fill_at_open_policy() {
    // Sell stop at 100, market gaps down to open at 95.
    // FillAtOpen policy: fill at 95 (the open), not 100 (the trigger).
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

    let b = bar("SPY", 95.0, 97.0, 93.0, 96.0);
    let bars = bars_map(&b);
    let positions = HashMap::new();

    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, 95.0); // open, not trigger
}

#[test]
fn gap_through_sell_stop_fill_at_trigger_policy() {
    let config = ExecutionConfig {
        cost_model: CostModel::frictionless(),
        path_policy: PathPolicy::WorstCase,
        gap_policy: GapPolicy::FillAtTrigger,
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

    let b = bar("SPY", 95.0, 97.0, 93.0, 96.0);
    let bars = bars_map(&b);
    let positions = HashMap::new();

    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, 100.0); // trigger, not open
}

#[test]
fn gap_through_sell_stop_fill_at_worst_policy() {
    let config = ExecutionConfig {
        cost_model: CostModel::frictionless(),
        path_policy: PathPolicy::WorstCase,
        gap_policy: GapPolicy::FillAtWorst,
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

    // Gap down: open at 95 < trigger at 100. Worst for a sell = lower price = 95.
    let b = bar("SPY", 95.0, 97.0, 93.0, 96.0);
    let bars = bars_map(&b);
    let positions = HashMap::new();

    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    assert_eq!(fills.len(), 1);
    // For sell stop gap-through, worst = min(open, trigger) = 95
    assert_eq!(fills[0].price, 95.0);
}

#[test]
fn gap_through_buy_stop_fill_at_open() {
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
        OrderSide::Buy,
        OrderType::StopMarket {
            trigger_price: 100.0,
        },
    ));

    // Gap up: open at 105 > trigger at 100
    let b = bar("SPY", 105.0, 110.0, 103.0, 108.0);
    let bars = bars_map(&b);
    let positions = HashMap::new();

    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, 105.0); // filled at open
}

// ─── Ambiguous bar / path policy tests ───────────────────────────────

#[test]
fn ambiguous_bar_worst_case_stop_fills_before_tp() {
    let config = ExecutionConfig {
        cost_model: CostModel::frictionless(),
        path_policy: PathPolicy::WorstCase,
        gap_policy: GapPolicy::FillAtOpen,
        liquidity: None,
    };
    let engine = ExecutionEngine::new(config);
    let mut book = OrderBook::new();

    // OCO pair for a long position: stop-loss and take-profit
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

    let oco = trendlab_core::domain::OcoGroup {
        id: OcoGroupId(10),
        order_ids: vec![OrderId(1), OrderId(2)],
    };
    book.submit(stop);
    book.submit(tp);
    book.register_oco_group(oco);

    // Bar where both are reachable: low=94 (stop hits), high=112 (tp hits)
    let b = bar("SPY", 100.0, 112.0, 94.0, 105.0);
    let bars = bars_map(&b);
    let mut positions = HashMap::new();
    positions.insert("SPY".into(), PositionSide::Long);

    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);

    // WorstCase for long: stop fills first, TP cancelled
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].order_id, OrderId(1)); // stop
    assert_eq!(fills[0].price, 95.0);

    // TP should be cancelled via OCO
    assert!(matches!(
        book.get(OrderId(2)).unwrap().status,
        OrderStatus::Cancelled { .. }
    ));
}

#[test]
fn ambiguous_bar_best_case_tp_fills_before_stop() {
    let config = ExecutionConfig {
        cost_model: CostModel::frictionless(),
        path_policy: PathPolicy::BestCase,
        gap_policy: GapPolicy::FillAtOpen,
        liquidity: None,
    };
    let engine = ExecutionEngine::new(config);
    let mut book = OrderBook::new();

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

    let oco = trendlab_core::domain::OcoGroup {
        id: OcoGroupId(10),
        order_ids: vec![OrderId(1), OrderId(2)],
    };
    book.submit(stop);
    book.submit(tp);
    book.register_oco_group(oco);

    let b = bar("SPY", 100.0, 112.0, 94.0, 105.0);
    let bars = bars_map(&b);
    let mut positions = HashMap::new();
    positions.insert("SPY".into(), PositionSide::Long);

    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);

    // BestCase for long: TP fills first, stop cancelled
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].order_id, OrderId(2)); // tp
    assert_eq!(fills[0].price, 110.0);

    // Stop should be cancelled via OCO
    assert!(matches!(
        book.get(OrderId(1)).unwrap().status,
        OrderStatus::Cancelled { .. }
    ));
}

// ─── Preset comparison ───────────────────────────────────────────────

#[test]
fn four_presets_produce_ordered_buy_prices() {
    // For a buy order, increasing friction means increasing fill price.
    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let presets = [
        ExecutionPreset::Frictionless,
        ExecutionPreset::Optimistic,
        ExecutionPreset::Realistic,
        ExecutionPreset::Hostile,
    ];

    let mut prices = Vec::new();
    for preset in &presets {
        let engine = ExecutionEngine::from_preset(*preset);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

        let bars = bars_map(&b);
        let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
        prices.push(fills[0].price);
    }

    // Frictionless <= Optimistic <= Realistic <= Hostile
    for i in 0..prices.len() - 1 {
        assert!(
            prices[i] <= prices[i + 1],
            "Expected {} ({:?}) <= {} ({:?})",
            prices[i],
            presets[i],
            prices[i + 1],
            presets[i + 1]
        );
    }
}

#[test]
fn four_presets_produce_ordered_sell_prices() {
    // For a sell order, increasing friction means decreasing fill price.
    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let presets = [
        ExecutionPreset::Frictionless,
        ExecutionPreset::Optimistic,
        ExecutionPreset::Realistic,
        ExecutionPreset::Hostile,
    ];

    let mut prices = Vec::new();
    for preset in &presets {
        let engine = ExecutionEngine::from_preset(*preset);
        let mut book = OrderBook::new();
        book.submit(make_order(1, OrderSide::Sell, OrderType::MarketOnOpen));

        let bars = bars_map(&b);
        let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
        prices.push(fills[0].price);
    }

    // Frictionless >= Optimistic >= Realistic >= Hostile (sell gets less with more friction)
    for i in 0..prices.len() - 1 {
        assert!(
            prices[i] >= prices[i + 1],
            "Expected {} ({:?}) >= {} ({:?})",
            prices[i],
            presets[i],
            prices[i + 1],
            presets[i + 1]
        );
    }
}

// ─── Directional slippage ────────────────────────────────────────────

#[test]
fn buy_slippage_increases_fill_price() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Realistic);
    let mut book = OrderBook::new();
    book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let bars = bars_map(&b);

    let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
    assert!(fills[0].price > 100.0, "buy slippage should increase price");
    assert!(fills[0].slippage > 0.0);
}

#[test]
fn sell_slippage_decreases_fill_price() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Realistic);
    let mut book = OrderBook::new();
    book.submit(make_order(1, OrderSide::Sell, OrderType::MarketOnOpen));

    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let bars = bars_map(&b);

    let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
    assert!(
        fills[0].price < 100.0,
        "sell slippage should decrease price"
    );
    assert!(fills[0].slippage > 0.0);
}

// ─── Same-bar entry+exit prevention ──────────────────────────────────

#[test]
fn bracket_stop_does_not_fill_on_entry_bar() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
    let mut book = OrderBook::new();

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

    // Bar where stop is reachable (low=94 < stop=95)
    let b = bar("SPY", 100.0, 105.0, 94.0, 103.0);
    let bars = bars_map(&b);

    // Phase 1: entry fills, children activate with activated_bar = 0
    let start_fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
    assert_eq!(start_fills.len(), 1);
    assert_eq!(start_fills[0].order_id, OrderId(1));
    assert_eq!(book.get(OrderId(2)).unwrap().activated_bar, Some(0));

    // Phase 2: stop should NOT fill on same bar (bar_index=0)
    let positions = HashMap::new();
    let intrabar_fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    assert!(
        intrabar_fills.is_empty(),
        "bracket child must not fill same bar as entry"
    );
    assert!(book.get(OrderId(2)).unwrap().is_active()); // still pending

    // Phase 2 on NEXT bar: stop should fill
    let b2 = bar("SPY", 96.0, 97.0, 93.0, 94.0);
    let bars2 = bars_map(&b2);
    let intrabar_fills2 = engine.process_intrabar(&mut book, &bars2, &instruments(), 1, &positions);
    assert_eq!(intrabar_fills2.len(), 1);
    assert_eq!(intrabar_fills2[0].order_id, OrderId(2));
}

// ─── Fill phases ─────────────────────────────────────────────────────

#[test]
fn fill_phases_are_correctly_tagged() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let bars = bars_map(&b);

    // MOO → StartOfBar
    let mut book = OrderBook::new();
    book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));
    let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
    assert_eq!(fills[0].phase, FillPhase::StartOfBar);

    // Stop → Intrabar
    let mut book = OrderBook::new();
    book.submit(make_order(
        2,
        OrderSide::Sell,
        OrderType::StopMarket {
            trigger_price: 99.0,
        },
    ));
    let positions = HashMap::new();
    let fills = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    assert_eq!(fills[0].phase, FillPhase::Intrabar);

    // MOC → EndOfBar
    let mut book = OrderBook::new();
    book.submit(make_order(3, OrderSide::Sell, OrderType::MarketOnClose));
    let fills = engine.process_end_of_bar(&mut book, &bars, &instruments(), 0);
    assert_eq!(fills[0].phase, FillPhase::EndOfBar);
}

// ─── Portfolio integration (full round trip) ─────────────────────────

#[test]
fn full_round_trip_equity_accounting() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
    let mut book = OrderBook::new();
    let mut portfolio = Portfolio::new(100_000.0);

    // Buy 100 shares at open
    book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));

    let b1 = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let bars1 = bars_map(&b1);

    let fills = engine.process_start_of_bar(&mut book, &bars1, &instruments(), 0);
    apply_fills(&fills, &mut portfolio);

    // Cash: 100000 - 100*100 = 90000
    assert!((portfolio.cash - 90_000.0).abs() < 1e-10);
    let pos = portfolio.get_position("SPY").unwrap();
    assert_eq!(pos.quantity, 100.0);
    assert_eq!(pos.side, PositionSide::Long);

    // Equity at bar close (103.0): 90000 + 100*103 = 100300
    let mut prices = HashMap::new();
    prices.insert("SPY".into(), 103.0);
    let equity = portfolio.equity(&prices);
    assert!((equity - 100_300.0).abs() < 1e-10);

    // Sell 100 shares at open of next bar
    book.submit(make_order(2, OrderSide::Sell, OrderType::MarketOnOpen));
    let b2 = bar("SPY", 110.0, 112.0, 108.0, 111.0);
    let bars2 = bars_map(&b2);

    let fills = engine.process_start_of_bar(&mut book, &bars2, &instruments(), 1);
    apply_fills(&fills, &mut portfolio);

    // Cash: 90000 + 100*110 = 101000
    assert!((portfolio.cash - 101_000.0).abs() < 1e-10);
    // Position should be flat
    assert!(!portfolio.has_position("SPY"));

    // Final equity = all cash
    let equity_final = portfolio.equity(&HashMap::new());
    assert!((equity_final - 101_000.0).abs() < 1e-10);
}

#[test]
fn round_trip_with_commission_and_slippage() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Realistic);
    let mut book = OrderBook::new();
    let mut portfolio = Portfolio::new(100_000.0);

    // Buy at open with friction
    book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));
    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let bars = bars_map(&b);
    let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);

    assert!(fills[0].slippage > 0.0);
    assert!(fills[0].commission > 0.0);
    apply_fills(&fills, &mut portfolio);

    // Sell at open with friction
    book.submit(make_order(2, OrderSide::Sell, OrderType::MarketOnOpen));
    let b2 = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let bars2 = bars_map(&b2);
    let fills = engine.process_start_of_bar(&mut book, &bars2, &instruments(), 1);

    assert!(fills[0].slippage > 0.0);
    assert!(fills[0].commission > 0.0);
    apply_fills(&fills, &mut portfolio);

    // Bought high, sold low due to slippage → should lose money
    assert!(portfolio.cash < 100_000.0, "friction should cost money");
    assert!(portfolio.total_commission > 0.0);
    assert!(portfolio.total_slippage > 0.0);
}

// ─── Liquidity constraint ────────────────────────────────────────────

#[test]
fn liquidity_constraint_limits_fill_quantity() {
    let config = ExecutionConfig {
        cost_model: CostModel::frictionless(),
        path_policy: PathPolicy::WorstCase,
        gap_policy: GapPolicy::FillAtOpen,
        liquidity: Some(LiquidityPolicy::new(0.01, RemainderPolicy::Cancel)), // 1% participation
    };
    let engine = ExecutionEngine::new(config);
    let mut book = OrderBook::new();

    // Want to buy 100 shares, but bar volume is 1000 → 1% = 10 shares max
    let mut order = make_order(1, OrderSide::Buy, OrderType::MarketOnOpen);
    order.quantity = 100.0;
    book.submit(order);

    let b = Bar {
        symbol: "SPY".into(),
        date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        open: 100.0,
        high: 105.0,
        low: 98.0,
        close: 103.0,
        volume: 1_000,
        adj_close: 103.0,
    };
    let bars = bars_map(&b);

    let fills = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
    assert_eq!(fills.len(), 1);
    assert!((fills[0].quantity - 10.0).abs() < 1e-10); // 1% of 1000
}

// ─── Void bar handling ───────────────────────────────────────────────

#[test]
fn void_bar_produces_no_fills() {
    let engine = ExecutionEngine::from_preset(ExecutionPreset::Frictionless);
    let mut book = OrderBook::new();
    book.submit(make_order(1, OrderSide::Buy, OrderType::MarketOnOpen));
    book.submit(make_order(
        2,
        OrderSide::Sell,
        OrderType::StopMarket {
            trigger_price: 95.0,
        },
    ));
    book.submit(make_order(3, OrderSide::Sell, OrderType::MarketOnClose));

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
    let bars = bars_map(&b);
    let positions = HashMap::new();

    let f1 = engine.process_start_of_bar(&mut book, &bars, &instruments(), 0);
    let f2 = engine.process_intrabar(&mut book, &bars, &instruments(), 0, &positions);
    let f3 = engine.process_end_of_bar(&mut book, &bars, &instruments(), 0);

    assert!(f1.is_empty());
    assert!(f2.is_empty());
    assert!(f3.is_empty());

    // All orders should still be active
    assert!(book.get(OrderId(1)).unwrap().is_active());
    assert!(book.get(OrderId(2)).unwrap().is_active());
    assert!(book.get(OrderId(3)).unwrap().is_active());
}

// ─── Execution model types ───────────────────────────────────────────

#[test]
fn all_four_execution_models_produce_correct_order_types() {
    use trendlab_core::components::execution::{
        CloseOnSignalModel, ExecutionModel, LimitEntryModel, NextBarOpenModel, StopEntryModel,
    };
    use trendlab_core::components::signal::{SignalDirection, SignalEvent};
    use trendlab_core::domain::ids::SignalEventId;

    let signal = SignalEvent {
        id: SignalEventId(1),
        bar_index: 10,
        date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
        symbol: "SPY".into(),
        direction: SignalDirection::Long,
        strength: 0.8,
        metadata: {
            let mut m = HashMap::new();
            m.insert("breakout_level".into(), 106.0);
            m
        },
    };

    let b = bar("SPY", 100.0, 105.0, 98.0, 103.0);
    let inst = Instrument::us_equity("SPY");

    // NextBarOpen → MOO
    let nbo = NextBarOpenModel::default();
    assert!(matches!(
        nbo.entry_order_type(&signal, &b, &inst),
        OrderType::MarketOnOpen
    ));

    // StopEntry → StopMarket
    let se = StopEntryModel::default();
    match se.entry_order_type(&signal, &b, &inst) {
        OrderType::StopMarket { trigger_price } => assert_eq!(trigger_price, 106.0),
        other => panic!("expected StopMarket, got {:?}", other),
    }

    // LimitEntry → Limit
    let le = LimitEntryModel::default();
    match le.entry_order_type(&signal, &b, &inst) {
        OrderType::Limit { limit_price } => assert!(limit_price < 103.0),
        other => panic!("expected Limit, got {:?}", other),
    }

    // CloseOnSignal → MOC
    let cos = CloseOnSignalModel::default();
    assert!(matches!(
        cos.entry_order_type(&signal, &b, &inst),
        OrderType::MarketOnClose
    ));
}
