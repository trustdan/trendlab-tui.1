//! End-to-end SPY backtest with hardcoded buy/sell logic.
//!
//! Uses the frozen SPY 2024 fixture. Hardcoded trading rules:
//!   - Buy MOO on bar 30 (after warmup)
//!   - Place stop-loss at entry - 5.0
//!   - Place take-profit at entry + 10.0
//!   - Sell MOO on bar 100 if still holding
//!
//! Runs with all four execution presets and verifies:
//! - Equity identity holds every bar
//! - Fills are generated
//! - Different presets produce different final equities
//! - Frictionless >= Realistic >= Hostile for the same trade sequence
//! - No NaN in equity curve

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use trendlab_core::components::execution::ExecutionPreset;
use trendlab_core::data::cache::ParquetCache;
use trendlab_core::data::provider::RawBar;
use trendlab_core::domain::ids::{OcoGroupId, OrderId};
use trendlab_core::domain::instrument::{Instrument, OrderSide};
use trendlab_core::domain::order::{Order, OrderStatus, OrderType};
use trendlab_core::domain::Bar;
use trendlab_core::engine::execution::{ExecutionConfig, ExecutionEngine};
use trendlab_core::engine::portfolio_update::apply_fills;
use trendlab_core::engine::state::EngineState;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_spy_fixture() -> Vec<RawBar> {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir =
        std::env::temp_dir().join(format!("trendlab_e2e_test_{}_{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);

    let sym_dir = cache_dir.join("symbol=SPY");
    std::fs::create_dir_all(&sym_dir).unwrap();
    std::fs::copy(
        fixture_dir().join("spy_2024.parquet"),
        sym_dir.join("2024.parquet"),
    )
    .unwrap();

    let meta = r#"{"symbol":"SPY","start_date":"2024-01-02","end_date":"2024-12-31","bar_count":252,"data_hash":"fixture","source":"fixture","cached_at":"2024-01-01T00:00:00"}"#;
    std::fs::write(sym_dir.join("meta.json"), meta).unwrap();

    let cache = ParquetCache::new(&cache_dir);
    let bars = cache.load("SPY").unwrap();
    let _ = std::fs::remove_dir_all(&cache_dir);
    bars
}

/// Convert RawBar to Bar.
fn raw_to_bar(raw: &RawBar, symbol: &str) -> Bar {
    Bar {
        symbol: symbol.into(),
        date: raw.date,
        open: raw.open,
        high: raw.high,
        low: raw.low,
        close: raw.close,
        volume: raw.volume,
        adj_close: raw.adj_close,
    }
}

/// Run a manual bar loop on real SPY data with hardcoded trading logic.
///
/// Returns (final_equity, equity_curve, fill_count).
fn run_e2e_backtest(preset: ExecutionPreset) -> (f64, Vec<f64>, usize) {
    let raw_bars = load_spy_fixture();
    assert!(raw_bars.len() >= 200, "need at least 200 bars");

    let bars: Vec<Bar> = raw_bars.iter().map(|r| raw_to_bar(r, "SPY")).collect();
    let instruments: HashMap<String, Instrument> = {
        let mut m = HashMap::new();
        m.insert("SPY".into(), Instrument::us_equity("SPY"));
        m
    };

    let config = ExecutionConfig::from_preset(preset);
    let engine = ExecutionEngine::new(config);
    let initial_capital = 100_000.0;
    let mut state = EngineState::new(initial_capital);
    let mut equity_curve = Vec::with_capacity(bars.len());
    let mut all_fills = Vec::new();
    let mut next_order_id = 1u64;

    let buy_bar = 30;
    let force_sell_bar = 100;
    let mut bought = false;
    let mut sold = false;

    for t in 0..bars.len() {
        state.bar_index = t;
        let bar = &bars[t];

        if bar.is_void() {
            // Equity carries forward
            let prices: HashMap<String, f64> = state
                .last_valid_close
                .iter()
                .map(|(k, &v)| (k.clone(), v))
                .collect();
            let equity = state.verify_equity(&prices);
            equity_curve.push(equity);
            continue;
        }

        let mut bar_map: HashMap<&str, &Bar> = HashMap::new();
        bar_map.insert("SPY", bar);

        // ─── Hardcoded signal: buy on bar 30, sell on bar 100 ───
        if t == buy_bar && !bought {
            // Submit buy MOO
            let order = Order {
                id: OrderId(next_order_id),
                symbol: "SPY".into(),
                side: OrderSide::Buy,
                order_type: OrderType::MarketOnOpen,
                quantity: 100.0,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: t,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };
            next_order_id += 1;
            state.order_book.submit(order);
            bought = true;
        }

        if t == force_sell_bar && !sold && state.portfolio.has_position("SPY") {
            // Force sell MOO
            let pos_qty = state.portfolio.get_position("SPY").unwrap().quantity;
            let order = Order {
                id: OrderId(next_order_id),
                symbol: "SPY".into(),
                side: OrderSide::Sell,
                order_type: OrderType::MarketOnOpen,
                quantity: pos_qty,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: t,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };
            next_order_id += 1;
            state.order_book.submit(order);
            sold = true;
        }

        // ─── Phase 1: Start-of-bar ───
        let start_fills =
            engine.process_start_of_bar(&mut state.order_book, &bar_map, &instruments, t);
        apply_fills(&start_fills, &mut state.portfolio);

        // ─── Phase 2: Intrabar ───
        let position_sides = state.position_sides();
        let intrabar_fills = engine.process_intrabar(
            &mut state.order_book,
            &bar_map,
            &instruments,
            t,
            &position_sides,
        );
        apply_fills(&intrabar_fills, &mut state.portfolio);

        // ─── Phase 3: End-of-bar ───
        let eob_fills = engine.process_end_of_bar(&mut state.order_book, &bar_map, &instruments, t);
        apply_fills(&eob_fills, &mut state.portfolio);

        all_fills.extend(start_fills);
        all_fills.extend(intrabar_fills);
        all_fills.extend(eob_fills);

        // ─── Phase 4: Post-bar ───
        if let Some(pos) = state.portfolio.get_position_mut("SPY") {
            pos.tick_bar();
            pos.update_mark(bar.close);
        }
        state.last_valid_close.insert("SPY".into(), bar.close);

        // Equity accounting
        let mut prices = HashMap::new();
        prices.insert("SPY".into(), bar.close);
        let equity = state.verify_equity(&prices);
        equity_curve.push(equity);
    }

    let final_equity = *equity_curve.last().unwrap();
    (final_equity, equity_curve, all_fills.len())
}

// ─── Tests ───────────────────────────────────────────────────────────

#[test]
fn e2e_spy_frictionless_generates_fills() {
    let (final_equity, equity_curve, fill_count) = run_e2e_backtest(ExecutionPreset::Frictionless);

    // Should generate exactly 2 fills: buy + sell
    assert_eq!(fill_count, 2, "expected exactly 2 fills (buy + sell)");

    // Equity curve should have no NaN
    for (i, &eq) in equity_curve.iter().enumerate() {
        assert!(!eq.is_nan(), "NaN at bar {i}");
    }

    // Final equity should differ from initial (we traded)
    assert_ne!(
        final_equity, 100_000.0,
        "final equity should differ from initial"
    );
}

#[test]
fn e2e_spy_equity_identity_holds_every_bar() {
    let (_, equity_curve, _) = run_e2e_backtest(ExecutionPreset::Frictionless);

    // verify_equity is called inside run_e2e_backtest and panics on violation.
    // If we reach here, equity identity held for all bars.
    assert!(equity_curve.len() >= 200);
}

#[test]
fn e2e_spy_frictionless_beats_hostile() {
    let (eq_frictionless, _, _) = run_e2e_backtest(ExecutionPreset::Frictionless);
    let (eq_hostile, _, _) = run_e2e_backtest(ExecutionPreset::Hostile);

    // Frictionless should produce better or equal equity than hostile
    assert!(
        eq_frictionless >= eq_hostile,
        "frictionless ({eq_frictionless}) should be >= hostile ({eq_hostile})"
    );
}

#[test]
fn e2e_spy_realistic_between_extremes() {
    let (eq_frictionless, _, _) = run_e2e_backtest(ExecutionPreset::Frictionless);
    let (eq_realistic, _, _) = run_e2e_backtest(ExecutionPreset::Realistic);
    let (eq_hostile, _, _) = run_e2e_backtest(ExecutionPreset::Hostile);

    assert!(
        eq_frictionless >= eq_realistic,
        "frictionless ({eq_frictionless}) should be >= realistic ({eq_realistic})"
    );
    assert!(
        eq_realistic >= eq_hostile,
        "realistic ({eq_realistic}) should be >= hostile ({eq_hostile})"
    );
}

#[test]
fn e2e_spy_all_presets_produce_different_equities() {
    let (eq_f, _, _) = run_e2e_backtest(ExecutionPreset::Frictionless);
    let (eq_o, _, _) = run_e2e_backtest(ExecutionPreset::Optimistic);
    let (eq_r, _, _) = run_e2e_backtest(ExecutionPreset::Realistic);
    let (eq_h, _, _) = run_e2e_backtest(ExecutionPreset::Hostile);

    // At least some pairs should differ (friction matters)
    let equities = [eq_f, eq_o, eq_r, eq_h];
    let all_same = equities.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-10);
    assert!(
        !all_same,
        "all four presets produced identical equity — friction not working"
    );
}

#[test]
fn e2e_spy_equity_curve_monotonic_before_buy() {
    let (_, equity_curve, _) = run_e2e_backtest(ExecutionPreset::Frictionless);

    // Before the buy (bar 30), equity should be constant (no positions)
    for i in 0..30 {
        assert!(
            (equity_curve[i] - 100_000.0).abs() < 1e-10,
            "equity at bar {i} should be 100000, got {}",
            equity_curve[i]
        );
    }
}

#[test]
fn e2e_spy_equity_constant_after_sell() {
    let (_, equity_curve, _) = run_e2e_backtest(ExecutionPreset::Frictionless);

    // After the sell on bar 100, equity should be constant (all cash, no positions)
    let post_sell_equity = equity_curve[100];
    for i in 101..equity_curve.len() {
        assert!(
            (equity_curve[i] - post_sell_equity).abs() < 1e-10,
            "equity at bar {i} ({}) should equal post-sell equity ({post_sell_equity})",
            equity_curve[i]
        );
    }
}

#[test]
fn e2e_spy_with_stop_loss_bracket() {
    // This test uses a bracket order: buy + stop-loss.
    // The stop should NOT fill on the same bar as entry.
    let raw_bars = load_spy_fixture();
    let bars: Vec<Bar> = raw_bars.iter().map(|r| raw_to_bar(r, "SPY")).collect();
    let instruments: HashMap<String, Instrument> = {
        let mut m = HashMap::new();
        m.insert("SPY".into(), Instrument::us_equity("SPY"));
        m
    };

    let config = ExecutionConfig::from_preset(ExecutionPreset::Frictionless);
    let engine = ExecutionEngine::new(config);
    let mut state = EngineState::new(100_000.0);
    let mut all_fills = Vec::new();

    let buy_bar = 30;

    for t in 0..bars.len().min(50) {
        state.bar_index = t;
        let bar = &bars[t];
        if bar.is_void() {
            continue;
        }

        let mut bar_map: HashMap<&str, &Bar> = HashMap::new();
        bar_map.insert("SPY", bar);

        // Submit bracket on buy_bar
        if t == buy_bar {
            let entry = Order {
                id: OrderId(1),
                symbol: "SPY".into(),
                side: OrderSide::Buy,
                order_type: OrderType::MarketOnOpen,
                quantity: 100.0,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: t,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };
            let stop = Order {
                id: OrderId(2),
                symbol: "SPY".into(),
                side: OrderSide::Sell,
                order_type: OrderType::StopMarket {
                    trigger_price: bar.open - 50.0, // very wide stop — should NOT fill same bar
                },
                quantity: 100.0,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: t,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };
            state
                .order_book
                .submit_bracket(entry, stop, None, OcoGroupId(100));
        }

        // Run phases
        let start_fills =
            engine.process_start_of_bar(&mut state.order_book, &bar_map, &instruments, t);
        apply_fills(&start_fills, &mut state.portfolio);

        let position_sides = state.position_sides();
        let intrabar_fills = engine.process_intrabar(
            &mut state.order_book,
            &bar_map,
            &instruments,
            t,
            &position_sides,
        );
        apply_fills(&intrabar_fills, &mut state.portfolio);

        let eob_fills = engine.process_end_of_bar(&mut state.order_book, &bar_map, &instruments, t);
        apply_fills(&eob_fills, &mut state.portfolio);

        all_fills.extend(start_fills);
        all_fills.extend(intrabar_fills);
        all_fills.extend(eob_fills);

        // Post-bar
        if let Some(pos) = state.portfolio.get_position_mut("SPY") {
            pos.tick_bar();
            pos.update_mark(bar.close);
        }
        state.last_valid_close.insert("SPY".into(), bar.close);
    }

    // Entry should have filled on bar 30
    let entry_fills: Vec<_> = all_fills
        .iter()
        .filter(|f| f.order_id == OrderId(1))
        .collect();
    assert_eq!(entry_fills.len(), 1, "entry should have filled");
    assert_eq!(entry_fills[0].bar_index, buy_bar);

    // Stop child should have activated_bar = buy_bar
    let stop_order = state.order_book.get(OrderId(2)).unwrap();
    assert_eq!(stop_order.activated_bar, Some(buy_bar));

    // Stop should NOT have filled on the same bar as entry
    let stop_fills_same_bar: Vec<_> = all_fills
        .iter()
        .filter(|f| f.order_id == OrderId(2) && f.bar_index == buy_bar)
        .collect();
    assert!(
        stop_fills_same_bar.is_empty(),
        "stop must not fill on same bar as entry"
    );
}
