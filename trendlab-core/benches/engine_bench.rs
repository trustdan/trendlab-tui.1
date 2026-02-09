//! Criterion benchmarks for TrendLab hot paths.
//!
//! Benchmarks:
//! 1. Bar event loop (full backtest iteration)
//! 2. Order book operations (submit, fill, cancel/replace, OCO)
//! 3. Execution fill simulation (trigger checks, fill price computation)
//! 4. Indicator precompute (SMA, EMA, ATR, Donchian, Bollinger batch)
//! 5. Position manager state machine (sequential PM on_bar calls)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;

use trendlab_core::components::composition::build_composition;
use trendlab_core::components::execution::NextBarOpenModel;
use trendlab_core::components::filter::NoFilter;
use trendlab_core::components::indicator::Indicator;
use trendlab_core::components::pm::{AtrTrailing, NoOpPm, PositionManager};
use trendlab_core::components::signal::NullSignal;
use trendlab_core::data::align::AlignedData;
use trendlab_core::data::provider::RawBar;
use trendlab_core::domain::{
    Bar, Instrument, MarketStatus, OcoGroup, OcoGroupId, Order, OrderId, OrderSide, OrderStatus,
    OrderType, Position,
};
use trendlab_core::engine::order_book::OrderBook;
use trendlab_core::engine::precompute::precompute_indicators;
use trendlab_core::engine::{run_backtest, EngineConfig, ExecutionEngine};
use trendlab_core::fingerprint::TradingMode;
use trendlab_core::indicators::{Atr, Bollinger, Donchian, Ema, Sma};

// ── Helpers ──────────────────────────────────────────────────────────

fn make_raw_bars(n: usize) -> Vec<RawBar> {
    let base_date = chrono::NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();
    (0..n)
        .map(|i| {
            let close = 100.0 + (i as f64 * 0.1).sin() * 10.0;
            let open = close - 0.3;
            let high = close + 1.5;
            let low = close - 1.5;
            RawBar {
                date: base_date + chrono::Duration::days(i as i64),
                open,
                high,
                low,
                close,
                volume: 1_000_000 + (i as u64 % 500_000),
                adj_close: close,
            }
        })
        .collect()
}

fn make_bars(n: usize) -> Vec<Bar> {
    let base_date = chrono::NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();
    (0..n)
        .map(|i| {
            let close = 100.0 + (i as f64 * 0.1).sin() * 10.0;
            let open = close - 0.3;
            let high = close + 1.5;
            let low = close - 1.5;
            Bar {
                symbol: "BENCH".to_string(),
                date: base_date + chrono::Duration::days(i as i64),
                open,
                high,
                low,
                close,
                volume: 1_000_000 + (i as u64 % 500_000),
                adj_close: close,
            }
        })
        .collect()
}

fn make_aligned(n: usize) -> AlignedData {
    let raw = make_raw_bars(n);
    let dates = raw.iter().map(|b| b.date).collect();
    let mut bars = HashMap::new();
    bars.insert("BENCH".to_string(), raw);
    AlignedData {
        dates,
        bars,
        symbols: vec!["BENCH".to_string()],
    }
}

fn make_aligned_multi(n: usize, num_symbols: usize) -> AlignedData {
    let base_date = chrono::NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();
    let dates: Vec<_> = (0..n)
        .map(|i| base_date + chrono::Duration::days(i as i64))
        .collect();
    let symbols: Vec<String> = (0..num_symbols).map(|i| format!("SYM{i}")).collect();
    let mut bars = HashMap::new();
    for (si, sym) in symbols.iter().enumerate() {
        let raw: Vec<RawBar> = (0..n)
            .map(|i| {
                let close = 100.0 + (si as f64 * 10.0) + (i as f64 * 0.1).sin() * 10.0;
                RawBar {
                    date: dates[i],
                    open: close - 0.3,
                    high: close + 1.5,
                    low: close - 1.5,
                    close,
                    volume: 1_000_000,
                    adj_close: close,
                }
            })
            .collect();
        bars.insert(sym.clone(), raw);
    }
    AlignedData {
        dates,
        bars,
        symbols,
    }
}

fn make_order(id: u64, order_type: OrderType) -> Order {
    Order {
        id: OrderId(id),
        symbol: "BENCH".to_string(),
        side: OrderSide::Buy,
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

// ── 1. Bar Event Loop ────────────────────────────────────────────────

fn bench_bar_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("bar_event_loop");

    for &bar_count in &[252, 1260, 2520] {
        let aligned = make_aligned(bar_count);
        let config = EngineConfig::new(100_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![
            Box::new(Sma::new(20)),
            Box::new(Ema::new(50)),
            Box::new(Atr::new(14)),
        ];

        group.bench_with_input(
            BenchmarkId::new("null_signal", bar_count),
            &bar_count,
            |b, _| {
                b.iter(|| {
                    run_backtest(
                        black_box(&aligned),
                        black_box(&indicators),
                        black_box(&config),
                        &NullSignal,
                        &NoFilter,
                        &NextBarOpenModel::default(),
                        &NoOpPm,
                    )
                });
            },
        );
    }

    // Multi-symbol benchmark (the realistic case)
    let aligned_10 = make_aligned_multi(1260, 10);
    let config = EngineConfig::new(100_000.0, 0);
    let indicators: Vec<Box<dyn Indicator>> = vec![
        Box::new(Sma::new(20)),
        Box::new(Ema::new(50)),
        Box::new(Atr::new(14)),
    ];
    group.bench_function("10_symbols_1260_bars", |b| {
        b.iter(|| {
            run_backtest(
                black_box(&aligned_10),
                black_box(&indicators),
                black_box(&config),
                &NullSignal,
                &NoFilter,
                &NextBarOpenModel::default(),
                &NoOpPm,
            )
        });
    });

    group.finish();
}

// ── 2. Order Book Operations ─────────────────────────────────────────

fn bench_order_book(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_book");

    group.bench_function("submit_100", |b| {
        b.iter(|| {
            let mut book = OrderBook::new();
            for i in 0..100u64 {
                book.submit(make_order(i, OrderType::MarketOnOpen));
            }
            black_box(&book);
        });
    });

    group.bench_function("submit_fill_100", |b| {
        b.iter(|| {
            let mut book = OrderBook::new();
            for i in 0..100u64 {
                book.submit(make_order(i, OrderType::MarketOnOpen));
                let _ = book.record_fill(OrderId(i), 100.0, 0);
            }
            black_box(&book);
        });
    });

    group.bench_function("cancel_replace_50", |b| {
        b.iter(|| {
            let mut book = OrderBook::new();
            // Submit 50 stop orders
            for i in 0..50u64 {
                book.submit(make_order(
                    i,
                    OrderType::StopMarket {
                        trigger_price: 95.0,
                    },
                ));
            }
            // Cancel/replace each one
            for i in 0..50u64 {
                let replacement = make_order(
                    100 + i,
                    OrderType::StopMarket {
                        trigger_price: 97.0,
                    },
                );
                let _ = book.cancel_replace(OrderId(i), replacement, 5);
            }
            black_box(&book);
        });
    });

    group.bench_function("oco_fill_cancel_20_pairs", |b| {
        b.iter(|| {
            let mut book = OrderBook::new();
            for pair in 0..20u64 {
                let id_a = pair * 2;
                let id_b = pair * 2 + 1;
                let group_id = OcoGroupId(pair);
                let mut oa = make_order(
                    id_a,
                    OrderType::StopMarket {
                        trigger_price: 95.0,
                    },
                );
                oa.oco_group_id = Some(group_id);
                let mut ob = make_order(
                    id_b,
                    OrderType::Limit {
                        limit_price: 110.0,
                    },
                );
                ob.oco_group_id = Some(group_id);

                book.submit(oa);
                book.submit(ob);
                book.register_oco_group(OcoGroup {
                    id: group_id,
                    order_ids: vec![OrderId(id_a), OrderId(id_b)],
                });

                book.trigger(OrderId(id_a), 0).unwrap();
                let _ = book.record_fill(OrderId(id_a), 100.0, 0);
            }
            black_box(&book);
        });
    });

    group.finish();
}

// ── 3. Execution Fill Simulation ─────────────────────────────────────

fn bench_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution_fill");

    let exec = ExecutionEngine::new(
        trendlab_core::engine::ExecutionConfig::frictionless(),
    );
    let instrument = Instrument::us_equity("BENCH");
    let mut instruments = HashMap::new();
    instruments.insert("BENCH".to_string(), instrument);

    group.bench_function("start_of_bar_10_moo", |b| {
        let bar = Bar {
            symbol: "BENCH".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 98.0,
            close: 103.0,
            volume: 1_000_000,
            adj_close: 103.0,
        };
        let mut bar_map = HashMap::new();
        bar_map.insert("BENCH", &bar);

        b.iter(|| {
            let mut book = OrderBook::new();
            for i in 0..10u64 {
                book.submit(make_order(i, OrderType::MarketOnOpen));
            }
            let fills = exec.process_start_of_bar(
                &mut book,
                black_box(&bar_map),
                &instruments,
                0,
            );
            black_box(fills);
        });
    });

    group.bench_function("intrabar_10_stops", |b| {
        let bar = Bar {
            symbol: "BENCH".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 93.0, // low enough to trigger stops at 95
            close: 96.0,
            volume: 1_000_000,
            adj_close: 96.0,
        };
        let mut bar_map = HashMap::new();
        bar_map.insert("BENCH", &bar);
        let mut position_sides = HashMap::new();
        position_sides.insert("BENCH".to_string(), trendlab_core::domain::PositionSide::Long);

        b.iter(|| {
            let mut book = OrderBook::new();
            for i in 0..10u64 {
                let order = Order {
                    id: OrderId(i),
                    symbol: "BENCH".to_string(),
                    side: OrderSide::Sell,
                    order_type: OrderType::StopMarket {
                        trigger_price: 95.0,
                    },
                    quantity: 100.0,
                    filled_quantity: 0.0,
                    status: OrderStatus::Pending,
                    created_bar: 0,
                    parent_id: None,
                    oco_group_id: None,
                    activated_bar: None,
                };
                book.submit(order);
            }
            let fills = exec.process_intrabar(
                &mut book,
                black_box(&bar_map),
                &instruments,
                0,
                &position_sides,
            );
            black_box(fills);
        });
    });

    group.finish();
}

// ── 4. Indicator Precompute ──────────────────────────────────────────

fn bench_indicators(c: &mut Criterion) {
    let mut group = c.benchmark_group("indicator_precompute");

    for &bar_count in &[252, 1260, 2520] {
        let bars = make_bars(bar_count);
        let mut bars_by_symbol = HashMap::new();
        bars_by_symbol.insert("BENCH".to_string(), bars);

        // Single indicator
        let sma_indicators: Vec<Box<dyn Indicator>> = vec![Box::new(Sma::new(20))];
        group.bench_with_input(
            BenchmarkId::new("sma_20", bar_count),
            &bar_count,
            |b, _| {
                b.iter(|| {
                    precompute_indicators(black_box(&bars_by_symbol), black_box(&sma_indicators))
                });
            },
        );

        // Full indicator stack (typical strategy)
        let full_stack: Vec<Box<dyn Indicator>> = vec![
            Box::new(Sma::new(20)),
            Box::new(Sma::new(50)),
            Box::new(Ema::new(10)),
            Box::new(Ema::new(50)),
            Box::new(Atr::new(14)),
            Box::new(Donchian::upper(50)),
            Box::new(Donchian::lower(50)),
            Box::new(Bollinger::upper(20, 2.0)),
            Box::new(Bollinger::lower(20, 2.0)),
        ];
        group.bench_with_input(
            BenchmarkId::new("full_stack_9", bar_count),
            &bar_count,
            |b, _| {
                b.iter(|| {
                    precompute_indicators(black_box(&bars_by_symbol), black_box(&full_stack))
                });
            },
        );
    }

    group.finish();
}

// ── 5. Position Manager State Machine ────────────────────────────────

fn bench_pm_state_machine(c: &mut Criterion) {
    let mut group = c.benchmark_group("pm_state_machine");

    // ATR trailing PM: the sequential bottleneck mentioned in the plan.
    // This benchmarks the sequential PM on_bar path that runs once per
    // symbol per bar for every open position.
    let bars = make_bars(1260);
    let mut bars_by_symbol = HashMap::new();
    bars_by_symbol.insert("BENCH".to_string(), bars.clone());

    // Precompute indicators (ATR needed by pm)
    let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(Atr::new(14))];
    let indicator_values = precompute_indicators(&bars_by_symbol, &indicators);
    let iv = &indicator_values["BENCH"];

    group.bench_function("atr_trailing_1260_bars", |b| {
        let pm = AtrTrailing::new(14, 3.0);
        b.iter(|| {
            // Simulate a position held from bar 20 to end
            let mut pos = Position::new_long("BENCH".into(), bars[20].close, 100.0, 20);
            for t in 21..bars.len() {
                pos.tick_bar();
                pos.update_mark(bars[t].close);
                let intent = pm.on_bar(
                    black_box(&pos),
                    black_box(&bars[t]),
                    t,
                    MarketStatus::Open,
                    black_box(iv),
                );
                // Apply stop ratchet
                if let Some(stop) = intent.stop_price {
                    match pos.current_stop {
                        Some(cur) => {
                            pos.current_stop = Some(stop.max(cur));
                        }
                        None => {
                            pos.current_stop = Some(stop);
                        }
                    }
                }
                black_box(&intent);
            }
            black_box(&pos);
        });
    });

    group.finish();
}

// ── 6. Full Backtest with Strategy Composition ───────────────────────

fn bench_full_composition(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_composition");

    let aligned = make_aligned(1260);
    let config = trendlab_core::fingerprint::StrategyConfig {
        signal: trendlab_core::fingerprint::ComponentConfig {
            component_type: "donchian_breakout".into(),
            params: [("entry_lookback".to_string(), 50.0)].into_iter().collect(),
        },
        position_manager: trendlab_core::fingerprint::ComponentConfig {
            component_type: "atr_trailing".into(),
            params: [
                ("atr_period".to_string(), 14.0),
                ("multiplier".to_string(), 3.0),
            ]
            .into_iter()
            .collect(),
        },
        execution_model: trendlab_core::fingerprint::ComponentConfig {
            component_type: "next_bar_open".into(),
            params: [("preset".to_string(), 1.0)].into_iter().collect(),
        },
        signal_filter: trendlab_core::fingerprint::ComponentConfig {
            component_type: "no_filter".into(),
            params: Default::default(),
        },
    };

    let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
    let engine_config = EngineConfig::new(100_000.0, 0);

    group.bench_function("donchian_atr_1260_bars", |b| {
        b.iter(|| {
            run_backtest(
                black_box(&aligned),
                black_box(&comp.indicators),
                black_box(&engine_config),
                comp.signal.as_ref(),
                comp.filter.as_ref(),
                comp.execution.as_ref(),
                comp.pm.as_ref(),
            )
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_bar_loop,
    bench_order_book,
    bench_execution,
    bench_indicators,
    bench_pm_state_machine,
    bench_full_composition,
);
criterion_main!(benches);
