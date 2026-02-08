//! Bar-by-bar event loop — the heart of the backtesting engine.
//!
//! Four phases per bar:
//! 1. Start-of-bar: activate day orders, fill MOO orders
//! 2. Intrabar: simulate trigger checks for stop/limit orders
//! 3. End-of-bar: fill MOC orders
//! 4. Post-bar: mark-to-market, equity accounting, PM maintenance orders

use crate::components::execution::ExecutionModel;
use crate::components::filter::SignalFilter;
use crate::components::indicator::Indicator;
use crate::components::pm::{IntentAction, OrderIntent, PositionManager};
use crate::components::signal::{SignalDirection, SignalGenerator};
use crate::data::align::AlignedData;
use crate::domain::{Bar, Fill, MarketStatus, Order, OrderStatus, OrderType, PositionSide};
use crate::engine::execution::ExecutionEngine;
use crate::engine::portfolio_update::apply_fills;
use crate::engine::stickiness::compute_stickiness;

use super::convert::aligned_to_bars;
use super::precompute::{compute_warmup, precompute_indicators};
use super::state::{EngineConfig, EngineState, RunResult};
use super::trade_extraction::extract_trades;

use std::collections::HashMap;

/// Data quality threshold: warn if void bar rate exceeds this fraction.
const VOID_BAR_RATE_THRESHOLD: f64 = 0.10;

/// Run a backtest on aligned data.
///
/// This is the main entry point for the engine. It:
/// 1. Converts `AlignedData` to per-symbol `Vec<Bar>`
/// 2. Precomputes all indicators per symbol
/// 3. Computes warmup length from indicator lookbacks
/// 4. Runs the four-phase bar loop
/// 5. Returns `RunResult`
pub fn run_backtest(
    aligned: &AlignedData,
    indicators: &[Box<dyn Indicator>],
    config: &EngineConfig,
    signal_generator: &dyn SignalGenerator,
    signal_filter: &dyn SignalFilter,
    execution_model: &dyn ExecutionModel,
    position_manager: &dyn PositionManager,
) -> RunResult {
    // Step 1: Convert RawBar → Bar
    let bars_by_symbol = aligned_to_bars(aligned);
    let symbols: Vec<&str> = aligned.symbols.iter().map(|s| s.as_str()).collect();
    let num_bars = aligned.dates.len();

    // Step 2: Precompute indicators
    let indicator_values = precompute_indicators(&bars_by_symbol, indicators);

    // Step 3: Compute warmup
    let indicator_warmup = compute_warmup(indicators);
    let warmup_bars = config.warmup_bars.max(indicator_warmup);

    // Step 4: Initialize engine state and execution engine
    let mut state = EngineState::new(config.initial_capital);
    let execution_engine = ExecutionEngine::new(config.execution_config.clone());
    let mut equity_curve = Vec::with_capacity(num_bars);
    let mut all_fills: Vec<Fill> = Vec::new();

    // Step 5: Run the bar loop
    for t in 0..num_bars {
        state.bar_index = t;

        // Determine market status per symbol for this bar
        let mut market_status: HashMap<&str, MarketStatus> = HashMap::new();
        for &symbol in &symbols {
            let bar = &bars_by_symbol[symbol][t];
            let status = if bar.is_void() {
                MarketStatus::Closed
            } else {
                MarketStatus::Open
            };
            market_status.insert(symbol, status);
        }

        // Build per-bar map for execution engine: HashMap<&str, &Bar>
        let mut bar_map: HashMap<&str, &Bar> = HashMap::new();
        for &symbol in &symbols {
            let bar = &bars_by_symbol[symbol][t];
            if market_status[symbol] == MarketStatus::Open {
                bar_map.insert(symbol, bar);
            }
        }

        // ─── Phase 1: Start-of-bar ───
        // Activate day orders, fill MOO and MarketImmediate orders.
        let start_fills = execution_engine.process_start_of_bar(
            &mut state.order_book,
            &bar_map,
            &config.instruments,
            t,
        );
        apply_fills(&start_fills, &mut state.portfolio);

        // ─── Phase 2: Intrabar ───
        // Check stop/limit triggers against bar's high/low range.
        let position_sides = state.position_sides();
        let intrabar_fills = execution_engine.process_intrabar(
            &mut state.order_book,
            &bar_map,
            &config.instruments,
            t,
            &position_sides,
        );
        apply_fills(&intrabar_fills, &mut state.portfolio);

        // ─── Phase 3: End-of-bar ───
        // Fill MOC orders at bar's close.
        let eob_fills = execution_engine.process_end_of_bar(
            &mut state.order_book,
            &bar_map,
            &config.instruments,
            t,
        );
        apply_fills(&eob_fills, &mut state.portfolio);

        // Collect all fills from this bar
        all_fills.extend(start_fills);
        all_fills.extend(intrabar_fills);
        all_fills.extend(eob_fills);

        // ─── Phase 4: Post-bar ───
        // Mark-to-market, update position statistics, equity accounting.
        for &symbol in &symbols {
            let bar = &bars_by_symbol[symbol][t];
            let status = market_status[symbol];

            // Track bar counts for data quality
            *state
                .total_bar_counts
                .entry(symbol.to_string())
                .or_default() += 1;
            if status == MarketStatus::Closed {
                *state.void_bar_counts.entry(symbol.to_string()).or_default() += 1;
            }

            // Update positions
            if let Some(pos) = state.portfolio.get_position_mut(symbol) {
                pos.tick_bar(); // Always increment, even on void bars

                match status {
                    MarketStatus::Open => {
                        // Mark-to-market at this bar's close
                        pos.update_mark(bar.close);
                        state.last_valid_close.insert(symbol.to_string(), bar.close);
                    }
                    MarketStatus::Closed => {
                        // Void bar: equity carries forward at last valid close.
                        // No mark-to-market update, no PnL change.
                        // PM time counters already incremented via tick_bar().
                    }
                }
            }

            // Track last valid close for equity calculation
            if status == MarketStatus::Open {
                state.last_valid_close.insert(symbol.to_string(), bar.close);
            }
        }

        // Equity accounting: build current prices for equity calculation
        let prices = build_current_prices(&bars_by_symbol, &state.last_valid_close, &symbols, t);
        let equity = state.verify_equity(&prices);
        equity_curve.push(equity);

        // Warmup check: skip signal evaluation and PM during warmup
        if t < warmup_bars {
            continue;
        }

        if !state.warmup_complete {
            state.warmup_complete = true;
        }

        // ─── Signal evaluation ───
        for &symbol in &symbols {
            if market_status[symbol] == MarketStatus::Closed {
                continue;
            }

            // Skip if already in a position for this symbol
            if state.portfolio.has_position(symbol) {
                continue;
            }

            let bars = &bars_by_symbol[symbol];
            let indicators_for_symbol = indicator_values
                .get(symbol)
                .expect("indicator values must exist for all symbols");

            // 1. Evaluate signal
            let mut signal = match signal_generator.evaluate(bars, t, indicators_for_symbol) {
                Some(s) => s,
                None => continue,
            };

            // Assign real signal ID
            signal.id = state.id_gen.next_signal_event_id();
            state.signal_count += 1;

            // 2. Trading mode filter
            match config.trading_mode {
                crate::fingerprint::TradingMode::LongOnly => {
                    if signal.direction == SignalDirection::Short {
                        continue;
                    }
                }
                crate::fingerprint::TradingMode::ShortOnly => {
                    if signal.direction == SignalDirection::Long {
                        continue;
                    }
                }
                crate::fingerprint::TradingMode::LongShort => {}
            }

            // 3. Apply signal filter
            let evaluation = signal_filter.evaluate(&signal, bars, t, indicators_for_symbol);
            let passed = evaluation.verdict.is_passed();
            state.signal_evaluations.push(evaluation);

            if !passed {
                continue;
            }

            // 4. Determine entry order type from execution model
            let instrument = config
                .instruments
                .get(symbol)
                .cloned()
                .unwrap_or_else(|| crate::domain::Instrument::us_equity(symbol));
            let bar = &bars[t];
            let order_type = execution_model.entry_order_type(&signal, bar, &instrument);

            // 5. Calculate quantity
            let equity = state.portfolio.cash; // simplified: use cash as sizing base
            let position_value = equity * config.position_size_pct;
            let quantity = if bar.close > 0.0 {
                (position_value / bar.close).floor().max(1.0)
            } else {
                continue;
            };

            // 6. Determine order side
            let order_side = match signal.direction {
                SignalDirection::Long => crate::domain::OrderSide::Buy,
                SignalDirection::Short => crate::domain::OrderSide::Sell,
            };

            // 7. Create and submit entry order
            let order_id = state.id_gen.next_order_id();
            let order = Order {
                id: order_id,
                symbol: symbol.to_string(),
                side: order_side,
                order_type,
                quantity,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: t,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };
            state.order_book.submit(order);

            // Track the entry signal for this symbol
            state.entry_signals.insert(symbol.to_string(), signal);
        }

        // ─── PM maintenance ───
        // For each symbol with an open position and Open status:
        //   1. Call position_manager.on_bar(position, bar, status, indicators)
        //   2. Enforce ratchet invariant
        //   3. Translate OrderIntent into cancel/replace on order book
        for &symbol in &symbols {
            if market_status[symbol] == MarketStatus::Closed {
                continue; // void bar: no PM evaluation
            }

            // Check if there's an open position. We need to clone the relevant
            // data to avoid borrow conflicts with state.
            let pm_input = {
                match state.portfolio.get_position(symbol) {
                    Some(pos) if !pos.is_flat() => Some((pos.clone(), pos.side)),
                    _ => None,
                }
            };

            let (pos_snapshot, side) = match pm_input {
                Some(data) => data,
                None => continue,
            };

            let bar = &bars_by_symbol[symbol][t];
            let indicators_for_symbol = indicator_values
                .get(symbol)
                .expect("indicator values must exist for all symbols");

            let raw_intent = position_manager.on_bar(
                &pos_snapshot,
                bar,
                t,
                market_status[symbol],
                indicators_for_symbol,
            );

            // Track PM calls for stickiness diagnostics
            state.pm_calls_total += 1;
            if raw_intent.action != IntentAction::Hold {
                state.pm_calls_active += 1;
            }

            // Enforce ratchet invariant
            let intent = enforce_ratchet(&raw_intent, &pos_snapshot);

            // Translate intent into order book operations
            apply_pm_intent(&intent, symbol, side, pos_snapshot.quantity, &mut state, t);
        }
    }

    // Extract round-trip trades from fills
    let all_trades = extract_trades(&all_fills, &bars_by_symbol, &state.entry_signals);

    // Build result
    let void_bar_rates = state.void_bar_rates();
    let mut data_quality_warnings = Vec::new();
    for (symbol, &rate) in &void_bar_rates {
        if rate > VOID_BAR_RATE_THRESHOLD {
            data_quality_warnings.push(format!(
                "{symbol}: {:.1}% void bars exceeds {:.0}% threshold",
                rate * 100.0,
                VOID_BAR_RATE_THRESHOLD * 100.0
            ));
        }
    }

    let final_equity = *equity_curve.last().unwrap_or(&config.initial_capital);
    let stickiness = compute_stickiness(&all_trades, state.pm_calls_total, state.pm_calls_active);

    RunResult {
        equity_curve,
        fills: all_fills,
        trades: all_trades,
        final_equity,
        bar_count: num_bars,
        warmup_bars,
        void_bar_rates,
        data_quality_warnings,
        stickiness,
        signal_count: state.signal_count,
        signal_evaluations: state.signal_evaluations,
    }
}

/// Enforce the ratchet invariant on a PM's order intent.
///
/// For longs: stops may only go UP (tighter = higher stop).
/// For shorts: stops may only go DOWN (tighter = lower stop).
/// In debug builds, a violation triggers a debug_assert. In release, silently clamps.
fn enforce_ratchet(intent: &OrderIntent, position: &crate::domain::Position) -> OrderIntent {
    match intent.action {
        IntentAction::AdjustStop => {
            let new_stop = match intent.stop_price {
                Some(p) => p,
                None => return OrderIntent::hold(),
            };

            match position.side {
                PositionSide::Long => {
                    let clamped = match position.current_stop {
                        Some(cur) => new_stop.max(cur),
                        None => new_stop,
                    };
                    OrderIntent::adjust_stop(clamped)
                }
                PositionSide::Short => {
                    let clamped = match position.current_stop {
                        Some(cur) => new_stop.min(cur),
                        None => new_stop,
                    };
                    OrderIntent::adjust_stop(clamped)
                }
                PositionSide::Flat => OrderIntent::hold(),
            }
        }
        _ => intent.clone(),
    }
}

/// Translate a PM intent into order book operations.
fn apply_pm_intent(
    intent: &OrderIntent,
    symbol: &str,
    side: PositionSide,
    quantity: f64,
    state: &mut EngineState,
    bar_index: usize,
) {
    let exit_side = match side {
        PositionSide::Long => crate::domain::OrderSide::Sell,
        PositionSide::Short => crate::domain::OrderSide::Buy,
        PositionSide::Flat => return,
    };

    match intent.action {
        IntentAction::Hold => { /* nothing */ }
        IntentAction::AdjustStop => {
            let stop_price = match intent.stop_price {
                Some(p) => p,
                None => return,
            };

            // Update position's current_stop
            if let Some(pos) = state.portfolio.get_position_mut(symbol) {
                pos.current_stop = Some(stop_price);
            }

            let new_order_id = state.id_gen.next_order_id();
            let new_stop_order = Order {
                id: new_order_id,
                symbol: symbol.to_string(),
                side: exit_side,
                order_type: OrderType::StopMarket {
                    trigger_price: stop_price,
                },
                quantity,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: bar_index,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };

            if let Some(&old_id) = state.stop_order_ids.get(symbol) {
                // Cancel/replace existing stop — check if old order is still active
                let old_is_active = state
                    .order_book
                    .get_order(old_id)
                    .is_some_and(|o| o.is_active());

                if old_is_active {
                    let _ = state
                        .order_book
                        .cancel_replace(old_id, new_stop_order, bar_index);
                } else {
                    // Old stop already filled/cancelled, submit fresh
                    state.order_book.submit(new_stop_order);
                }
            } else {
                // First stop placement
                state.order_book.submit(new_stop_order);
            }
            state
                .stop_order_ids
                .insert(symbol.to_string(), new_order_id);
        }
        IntentAction::ForceExit => {
            // Cancel existing stop if any
            if let Some(&old_id) = state.stop_order_ids.get(symbol) {
                let old_is_active = state
                    .order_book
                    .get_order(old_id)
                    .is_some_and(|o| o.is_active());
                if old_is_active {
                    let _ = state.order_book.cancel(old_id, bar_index, "PM force exit");
                }
            }
            state.stop_order_ids.remove(symbol);

            // Place MOO exit order for next bar
            let exit_order_id = state.id_gen.next_order_id();
            let exit_order = Order {
                id: exit_order_id,
                symbol: symbol.to_string(),
                side: exit_side,
                order_type: OrderType::MarketOnOpen,
                quantity,
                filled_quantity: 0.0,
                status: OrderStatus::Pending,
                created_bar: bar_index,
                parent_id: None,
                oco_group_id: None,
                activated_bar: None,
            };
            state.order_book.submit(exit_order);
        }
        IntentAction::AdjustTarget => {
            // Deferred: target management is not in the MVP PM set.
        }
    }
}

/// Build a price map for equity calculation at bar index `t`.
///
/// For open markets: use the bar's close price.
/// For closed/void markets: use the last valid close (carry forward).
fn build_current_prices(
    bars_by_symbol: &HashMap<String, Vec<Bar>>,
    last_valid_close: &HashMap<String, f64>,
    symbols: &[&str],
    bar_index: usize,
) -> HashMap<String, f64> {
    let mut prices = HashMap::new();
    for &symbol in symbols {
        let bar = &bars_by_symbol[symbol][bar_index];
        let price = if !bar.close.is_nan() {
            bar.close
        } else {
            // Void bar: use last valid close
            last_valid_close.get(symbol).copied().unwrap_or(0.0)
        };
        prices.insert(symbol.to_string(), price);
    }
    prices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::execution::NextBarOpenModel;
    use crate::components::filter::NoFilter;
    use crate::components::pm::NoOpPm;
    use crate::components::signal::NullSignal;
    use crate::data::align::AlignedData;
    use crate::data::provider::RawBar;
    use chrono::NaiveDate;

    fn make_aligned_single(bars: Vec<RawBar>) -> AlignedData {
        let dates: Vec<NaiveDate> = bars.iter().map(|b| b.date).collect();
        let mut bar_map = HashMap::new();
        bar_map.insert("SPY".to_string(), bars);
        AlignedData {
            dates,
            bars: bar_map,
            symbols: vec!["SPY".to_string()],
        }
    }

    fn simple_bars(n: usize) -> Vec<RawBar> {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        (0..n)
            .map(|i| {
                let close = 100.0 + i as f64;
                RawBar {
                    date: base_date + chrono::Duration::days(i as i64),
                    open: close - 0.5,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: 1000,
                    adj_close: close,
                }
            })
            .collect()
    }

    #[test]
    fn backtest_flat_portfolio_equity_constant() {
        let aligned = make_aligned_single(simple_bars(10));
        let config = EngineConfig::new(100_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![];

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        assert_eq!(result.bar_count, 10);
        assert_eq!(result.final_equity, 100_000.0);
        // No trades, no fills in Phase 5b
        assert!(result.fills.is_empty());
        assert!(result.trades.is_empty());
        // Equity should be constant (no positions)
        for &eq in &result.equity_curve {
            assert_eq!(eq, 100_000.0);
        }
    }

    #[test]
    fn backtest_warmup_respects_indicator_lookback() {
        let aligned = make_aligned_single(simple_bars(10));
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(crate::indicators::Sma::new(5))]; // lookback = 4
        let config = EngineConfig::new(100_000.0, 0); // no explicit warmup

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        // Warmup should be at least the indicator lookback (4)
        assert_eq!(result.warmup_bars, 4);
    }

    #[test]
    fn backtest_explicit_warmup_override() {
        let aligned = make_aligned_single(simple_bars(10));
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(crate::indicators::Sma::new(3))]; // lookback = 2
        let config = EngineConfig::new(100_000.0, 5); // explicit warmup = 5

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        // Explicit warmup (5) > indicator lookback (2)
        assert_eq!(result.warmup_bars, 5);
    }

    #[test]
    fn backtest_void_bar_handling() {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let mut bars = simple_bars(10);
        // Inject 3 void bars at indices 3, 4, 5
        for i in 3..=5 {
            bars[i] = RawBar {
                date: base_date + chrono::Duration::days(i as i64),
                open: f64::NAN,
                high: f64::NAN,
                low: f64::NAN,
                close: f64::NAN,
                volume: 0,
                adj_close: f64::NAN,
            };
        }

        let aligned = make_aligned_single(bars);
        let config = EngineConfig::new(100_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![];

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        // Void bar rate should be 3/10 = 30%
        let rate = result.void_bar_rates["SPY"];
        assert!((rate - 0.3).abs() < 1e-10);

        // Should have a data quality warning (30% > 10%)
        assert!(!result.data_quality_warnings.is_empty());
        assert!(result.data_quality_warnings[0].contains("SPY"));

        // Equity should still be constant (no positions)
        assert_eq!(result.final_equity, 100_000.0);
    }

    #[test]
    fn backtest_multi_symbol() {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let dates: Vec<NaiveDate> = (0..5)
            .map(|i| base_date + chrono::Duration::days(i as i64))
            .collect();

        let spy_bars: Vec<RawBar> = dates
            .iter()
            .enumerate()
            .map(|(i, &date)| {
                let close = 100.0 + i as f64;
                RawBar {
                    date,
                    open: close - 0.5,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: 1000,
                    adj_close: close,
                }
            })
            .collect();

        let mut qqq_bars: Vec<RawBar> = dates
            .iter()
            .enumerate()
            .map(|(i, &date)| {
                let close = 200.0 + i as f64;
                RawBar {
                    date,
                    open: close - 0.5,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: 2000,
                    adj_close: close,
                }
            })
            .collect();

        // QQQ has a void bar at index 2
        qqq_bars[2] = RawBar {
            date: dates[2],
            open: f64::NAN,
            high: f64::NAN,
            low: f64::NAN,
            close: f64::NAN,
            volume: 0,
            adj_close: f64::NAN,
        };

        let mut bar_map = HashMap::new();
        bar_map.insert("SPY".to_string(), spy_bars);
        bar_map.insert("QQQ".to_string(), qqq_bars);

        let aligned = AlignedData {
            dates,
            bars: bar_map,
            symbols: vec!["SPY".to_string(), "QQQ".to_string()],
        };

        let config = EngineConfig::new(100_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![];

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        assert_eq!(result.bar_count, 5);
        // SPY has 0 void bars
        assert!((result.void_bar_rates["SPY"]).abs() < 1e-10);
        // QQQ has 1/5 = 20% void bars
        assert!((result.void_bar_rates["QQQ"] - 0.2).abs() < 1e-10);
    }

    #[test]
    fn backtest_equity_curve_length_matches_bars() {
        let aligned = make_aligned_single(simple_bars(25));
        let config = EngineConfig::new(50_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![];

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        assert_eq!(result.equity_curve.len(), 25);
        assert_eq!(result.bar_count, 25);
    }

    #[test]
    fn backtest_with_indicators_precomputes() {
        let aligned = make_aligned_single(simple_bars(30));
        let config = EngineConfig::new(100_000.0, 0);
        let indicators: Vec<Box<dyn Indicator>> = vec![
            Box::new(crate::indicators::Sma::new(5)),
            Box::new(crate::indicators::Ema::new(10)),
        ];

        let result = run_backtest(
            &aligned,
            &indicators,
            &config,
            &NullSignal,
            &NoFilter,
            &NextBarOpenModel::default(),
            &NoOpPm,
        );

        // Should complete without panics, warmup = max(4, 9) = 9
        assert_eq!(result.warmup_bars, 9);
        assert_eq!(result.bar_count, 30);
    }

    #[test]
    fn enforce_ratchet_long_tightens() {
        let mut pos = crate::domain::Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.current_stop = Some(95.0);
        let intent = OrderIntent::adjust_stop(97.0); // tighter (higher)
        let result = enforce_ratchet(&intent, &pos);
        assert_eq!(result.stop_price, Some(97.0));
    }

    #[test]
    fn enforce_ratchet_long_clamps_loosening() {
        let mut pos = crate::domain::Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.current_stop = Some(95.0);
        let intent = OrderIntent::adjust_stop(90.0); // loosening (lower)
        let result = enforce_ratchet(&intent, &pos);
        assert_eq!(result.stop_price, Some(95.0)); // clamped to existing
    }

    #[test]
    fn enforce_ratchet_short_tightens() {
        let mut pos = crate::domain::Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.current_stop = Some(105.0);
        let intent = OrderIntent::adjust_stop(103.0); // tighter (lower)
        let result = enforce_ratchet(&intent, &pos);
        assert_eq!(result.stop_price, Some(103.0));
    }

    #[test]
    fn enforce_ratchet_short_clamps_loosening() {
        let mut pos = crate::domain::Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.current_stop = Some(105.0);
        let intent = OrderIntent::adjust_stop(110.0); // loosening (higher)
        let result = enforce_ratchet(&intent, &pos);
        assert_eq!(result.stop_price, Some(105.0)); // clamped
    }

    #[test]
    fn enforce_ratchet_no_existing_stop() {
        let pos = crate::domain::Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let intent = OrderIntent::adjust_stop(90.0);
        let result = enforce_ratchet(&intent, &pos);
        assert_eq!(result.stop_price, Some(90.0)); // no clamp, first stop
    }

    #[test]
    fn enforce_ratchet_force_exit_passes_through() {
        let pos = crate::domain::Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let intent = OrderIntent::force_exit();
        let result = enforce_ratchet(&intent, &pos);
        assert_eq!(result.action, IntentAction::ForceExit);
    }
}
