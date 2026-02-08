//! Trade extraction — converts raw fills into round-trip TradeRecord entries.
//!
//! Post-processes fills after the bar loop completes. Pure function:
//! fills + bar data + signal map → trade records.

use crate::components::signal::SignalEvent;
use crate::domain::instrument::OrderSide;
use crate::domain::position::PositionSide;
use crate::domain::{Bar, Fill, TradeRecord};
use std::collections::HashMap;

/// State for an open trade being tracked during extraction.
struct OpenTrade {
    symbol: String,
    side: PositionSide,
    entry_bar: usize,
    entry_date: chrono::NaiveDate,
    entry_price: f64,
    quantity: f64,
    entry_commission: f64,
    entry_slippage: f64,
}

/// Extract round-trip trades from fills and bar data.
///
/// Groups fills by symbol in chronological order. When a buy fill arrives with
/// no open long for that symbol, it's an entry. When a sell fill arrives closing
/// an open long, it's an exit (and vice versa for shorts).
///
/// MAE/MFE are computed by walking bar data between entry and exit.
pub fn extract_trades(
    fills: &[Fill],
    bars_by_symbol: &HashMap<String, Vec<Bar>>,
    entry_signals: &HashMap<String, SignalEvent>,
) -> Vec<TradeRecord> {
    let mut trades = Vec::new();
    let mut open_trades: HashMap<String, OpenTrade> = HashMap::new();

    for fill in fills {
        let symbol = &fill.symbol;

        if let Some(open) = open_trades.get(symbol) {
            // Check if this fill closes the open trade
            let is_exit = match open.side {
                PositionSide::Long => fill.side == OrderSide::Sell,
                PositionSide::Short => fill.side == OrderSide::Buy,
                PositionSide::Flat => false,
            };

            if is_exit {
                let open = open_trades.remove(symbol).unwrap();
                let trade = build_trade_record(
                    &open,
                    fill,
                    bars_by_symbol.get(symbol),
                    entry_signals.get(symbol),
                );
                trades.push(trade);
                continue;
            }
        }

        // This fill opens a new trade
        let side = match fill.side {
            OrderSide::Buy => PositionSide::Long,
            OrderSide::Sell => PositionSide::Short,
        };

        open_trades.insert(
            symbol.clone(),
            OpenTrade {
                symbol: symbol.clone(),
                side,
                entry_bar: fill.bar_index,
                entry_date: fill.date,
                entry_price: fill.price,
                quantity: fill.quantity,
                entry_commission: fill.commission,
                entry_slippage: fill.slippage,
            },
        );
    }

    trades
}

/// Build a TradeRecord from an open trade and an exit fill.
fn build_trade_record(
    open: &OpenTrade,
    exit_fill: &Fill,
    bars: Option<&Vec<Bar>>,
    signal: Option<&SignalEvent>,
) -> TradeRecord {
    let gross_pnl = match open.side {
        PositionSide::Long => (exit_fill.price - open.entry_price) * open.quantity,
        PositionSide::Short => (open.entry_price - exit_fill.price) * open.quantity,
        PositionSide::Flat => 0.0,
    };

    let commission = open.entry_commission + exit_fill.commission;
    let slippage = open.entry_slippage + exit_fill.slippage;
    let net_pnl = gross_pnl - commission - slippage;
    let bars_held = exit_fill.bar_index.saturating_sub(open.entry_bar);

    let (mae, mfe) = compute_mae_mfe(
        bars,
        open.entry_bar,
        exit_fill.bar_index,
        open.entry_price,
        open.quantity,
        open.side,
    );

    TradeRecord {
        symbol: open.symbol.clone(),
        side: open.side,
        entry_bar: open.entry_bar,
        entry_date: open.entry_date,
        entry_price: open.entry_price,
        exit_bar: exit_fill.bar_index,
        exit_date: exit_fill.date,
        exit_price: exit_fill.price,
        quantity: open.quantity,
        gross_pnl,
        commission,
        slippage,
        net_pnl,
        bars_held,
        mae,
        mfe,
        signal_id: signal.map(|s| s.id),
        signal_type: None, // Set by runner from composition info
        pm_type: None,
        execution_model: None,
        filter_type: None,
    }
}

/// Compute Maximum Adverse Excursion and Maximum Favorable Excursion
/// by walking bar data between entry and exit.
fn compute_mae_mfe(
    bars: Option<&Vec<Bar>>,
    entry_bar: usize,
    exit_bar: usize,
    entry_price: f64,
    quantity: f64,
    side: PositionSide,
) -> (f64, f64) {
    let bars = match bars {
        Some(b) => b,
        None => return (0.0, 0.0),
    };

    let mut worst_pnl = 0.0_f64;
    let mut best_pnl = 0.0_f64;

    let start = entry_bar.min(bars.len());
    let end = (exit_bar + 1).min(bars.len());

    for bar in &bars[start..end] {
        if bar.is_void() {
            continue;
        }

        // Check excursion at bar's low and high
        let (adverse_price, favorable_price) = match side {
            PositionSide::Long => (bar.low, bar.high),
            PositionSide::Short => (bar.high, bar.low),
            PositionSide::Flat => continue,
        };

        let adverse_pnl = match side {
            PositionSide::Long => (adverse_price - entry_price) * quantity,
            PositionSide::Short => (entry_price - adverse_price) * quantity,
            PositionSide::Flat => 0.0,
        };

        let favorable_pnl = match side {
            PositionSide::Long => (favorable_price - entry_price) * quantity,
            PositionSide::Short => (entry_price - favorable_price) * quantity,
            PositionSide::Flat => 0.0,
        };

        if adverse_pnl < worst_pnl {
            worst_pnl = adverse_pnl;
        }
        if favorable_pnl > best_pnl {
            best_pnl = favorable_pnl;
        }
    }

    (worst_pnl, best_pnl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fill::FillPhase;
    use crate::domain::ids::{OrderId, SignalEventId};
    use chrono::NaiveDate;

    fn make_bars(prices: &[(f64, f64, f64, f64)]) -> Vec<Bar> {
        let base = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        prices
            .iter()
            .enumerate()
            .map(|(i, &(o, h, l, c))| Bar {
                symbol: "SPY".into(),
                date: base + chrono::Duration::days(i as i64),
                open: o,
                high: h,
                low: l,
                close: c,
                volume: 1000,
                adj_close: c,
            })
            .collect()
    }

    fn buy_fill(symbol: &str, bar: usize, price: f64, qty: f64) -> Fill {
        Fill {
            order_id: OrderId(bar as u64),
            bar_index: bar,
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap() + chrono::Duration::days(bar as i64),
            symbol: symbol.into(),
            side: OrderSide::Buy,
            price,
            quantity: qty,
            commission: 0.0,
            slippage: 0.0,
            phase: FillPhase::StartOfBar,
        }
    }

    fn sell_fill(symbol: &str, bar: usize, price: f64, qty: f64) -> Fill {
        Fill {
            order_id: OrderId(bar as u64 + 100),
            bar_index: bar,
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap() + chrono::Duration::days(bar as i64),
            symbol: symbol.into(),
            side: OrderSide::Sell,
            price,
            quantity: qty,
            commission: 0.0,
            slippage: 0.0,
            phase: FillPhase::Intrabar,
        }
    }

    #[test]
    fn zero_fills_produces_zero_trades() {
        let trades = extract_trades(&[], &HashMap::new(), &HashMap::new());
        assert!(trades.is_empty());
    }

    #[test]
    fn single_long_round_trip() {
        let fills = vec![
            buy_fill("SPY", 2, 100.0, 50.0),
            sell_fill("SPY", 5, 110.0, 50.0),
        ];

        let bars = make_bars(&[
            (99.0, 101.0, 98.0, 100.0),   // bar 0
            (100.0, 102.0, 99.0, 101.0),  // bar 1
            (101.0, 103.0, 97.0, 100.0),  // bar 2 (entry)
            (100.0, 108.0, 96.0, 105.0),  // bar 3
            (105.0, 112.0, 100.0, 110.0), // bar 4
            (110.0, 115.0, 105.0, 110.0), // bar 5 (exit)
        ]);
        let mut bars_map = HashMap::new();
        bars_map.insert("SPY".to_string(), bars);

        let trades = extract_trades(&fills, &bars_map, &HashMap::new());

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert_eq!(t.symbol, "SPY");
        assert_eq!(t.side, PositionSide::Long);
        assert_eq!(t.entry_bar, 2);
        assert_eq!(t.exit_bar, 5);
        assert_eq!(t.entry_price, 100.0);
        assert_eq!(t.exit_price, 110.0);
        assert_eq!(t.quantity, 50.0);
        // gross_pnl = (110 - 100) * 50 = 500
        assert!((t.gross_pnl - 500.0).abs() < 1e-10);
        assert!((t.net_pnl - 500.0).abs() < 1e-10);
        assert_eq!(t.bars_held, 3);
        assert!(t.is_winner());
    }

    #[test]
    fn single_short_round_trip() {
        let fills = vec![
            sell_fill("SPY", 1, 100.0, 50.0),
            buy_fill("SPY", 4, 90.0, 50.0),
        ];

        let bars = make_bars(&[
            (100.0, 101.0, 99.0, 100.0),
            (100.0, 102.0, 98.0, 99.0), // bar 1 (entry sell)
            (99.0, 103.0, 95.0, 96.0),
            (96.0, 97.0, 88.0, 90.0),
            (90.0, 92.0, 88.0, 90.0), // bar 4 (exit buy)
        ]);
        let mut bars_map = HashMap::new();
        bars_map.insert("SPY".to_string(), bars);

        let trades = extract_trades(&fills, &bars_map, &HashMap::new());

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert_eq!(t.side, PositionSide::Short);
        // gross_pnl = (100 - 90) * 50 = 500
        assert!((t.gross_pnl - 500.0).abs() < 1e-10);
        assert_eq!(t.bars_held, 3);
    }

    #[test]
    fn multiple_sequential_trades_same_symbol() {
        let fills = vec![
            buy_fill("SPY", 1, 100.0, 50.0),
            sell_fill("SPY", 3, 105.0, 50.0),
            buy_fill("SPY", 5, 102.0, 50.0),
            sell_fill("SPY", 8, 108.0, 50.0),
        ];

        let trades = extract_trades(&fills, &HashMap::new(), &HashMap::new());

        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].entry_bar, 1);
        assert_eq!(trades[0].exit_bar, 3);
        assert_eq!(trades[1].entry_bar, 5);
        assert_eq!(trades[1].exit_bar, 8);
    }

    #[test]
    fn multi_symbol_interleaved() {
        let fills = vec![
            buy_fill("SPY", 1, 100.0, 50.0),
            buy_fill("QQQ", 2, 200.0, 25.0),
            sell_fill("SPY", 4, 105.0, 50.0),
            sell_fill("QQQ", 5, 210.0, 25.0),
        ];

        let trades = extract_trades(&fills, &HashMap::new(), &HashMap::new());

        assert_eq!(trades.len(), 2);
        let spy_trade = trades.iter().find(|t| t.symbol == "SPY").unwrap();
        let qqq_trade = trades.iter().find(|t| t.symbol == "QQQ").unwrap();

        assert!((spy_trade.gross_pnl - 250.0).abs() < 1e-10); // (105-100)*50
        assert!((qqq_trade.gross_pnl - 250.0).abs() < 1e-10); // (210-200)*25
    }

    #[test]
    fn commission_and_slippage_accounting() {
        let mut entry = buy_fill("SPY", 1, 100.0, 100.0);
        entry.commission = 5.0;
        entry.slippage = 2.0;
        let mut exit = sell_fill("SPY", 5, 110.0, 100.0);
        exit.commission = 5.0;
        exit.slippage = 2.0;

        let fills = vec![entry, exit];
        let trades = extract_trades(&fills, &HashMap::new(), &HashMap::new());

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert!((t.gross_pnl - 1000.0).abs() < 1e-10); // (110-100)*100
        assert!((t.commission - 10.0).abs() < 1e-10); // 5+5
        assert!((t.slippage - 4.0).abs() < 1e-10); // 2+2
        assert!((t.net_pnl - 986.0).abs() < 1e-10); // 1000 - 10 - 4
    }

    #[test]
    fn mae_mfe_long_trade() {
        let fills = vec![
            buy_fill("SPY", 1, 100.0, 10.0),
            sell_fill("SPY", 3, 105.0, 10.0),
        ];

        let bars = make_bars(&[
            (100.0, 101.0, 99.0, 100.0),  // bar 0
            (100.0, 101.0, 95.0, 98.0),   // bar 1 (entry): low=95 → adverse = (95-100)*10 = -50
            (98.0, 108.0, 97.0, 106.0),   // bar 2: high=108 → favorable = (108-100)*10 = 80
            (106.0, 107.0, 104.0, 105.0), // bar 3 (exit)
        ]);
        let mut bars_map = HashMap::new();
        bars_map.insert("SPY".to_string(), bars);

        let trades = extract_trades(&fills, &bars_map, &HashMap::new());

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        // MAE: worst low was 95 at bar 1 → (95-100)*10 = -50
        assert!((t.mae - (-50.0)).abs() < 1e-10);
        // MFE: best high was 108 at bar 2 → (108-100)*10 = 80
        assert!((t.mfe - 80.0).abs() < 1e-10);
    }

    #[test]
    fn mae_mfe_short_trade() {
        let fills = vec![
            sell_fill("SPY", 1, 100.0, 10.0),
            buy_fill("SPY", 3, 95.0, 10.0),
        ];

        let bars = make_bars(&[
            (100.0, 101.0, 99.0, 100.0),
            (100.0, 105.0, 98.0, 99.0), // bar 1: high=105 → adverse = (100-105)*10 = -50
            (99.0, 101.0, 92.0, 93.0),  // bar 2: low=92 → favorable = (100-92)*10 = 80
            (93.0, 96.0, 93.0, 95.0),
        ]);
        let mut bars_map = HashMap::new();
        bars_map.insert("SPY".to_string(), bars);

        let trades = extract_trades(&fills, &bars_map, &HashMap::new());

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert!((t.mae - (-50.0)).abs() < 1e-10);
        assert!((t.mfe - 80.0).abs() < 1e-10);
    }

    #[test]
    fn signal_traceability() {
        let fills = vec![
            buy_fill("SPY", 2, 100.0, 50.0),
            sell_fill("SPY", 5, 110.0, 50.0),
        ];

        let mut signals = HashMap::new();
        signals.insert(
            "SPY".to_string(),
            SignalEvent {
                id: SignalEventId(42),
                bar_index: 2,
                date: NaiveDate::from_ymd_opt(2024, 1, 4).unwrap(),
                symbol: "SPY".into(),
                direction: crate::components::signal::SignalDirection::Long,
                strength: 1.0,
                metadata: HashMap::new(),
            },
        );

        let trades = extract_trades(&fills, &HashMap::new(), &signals);

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert_eq!(t.signal_id, Some(SignalEventId(42)));
    }

    #[test]
    fn unmatched_entry_produces_no_trade() {
        // Only an entry, no exit
        let fills = vec![buy_fill("SPY", 2, 100.0, 50.0)];
        let trades = extract_trades(&fills, &HashMap::new(), &HashMap::new());
        assert!(trades.is_empty());
    }

    #[test]
    fn losing_trade_has_negative_pnl() {
        let fills = vec![
            buy_fill("SPY", 1, 100.0, 50.0),
            sell_fill("SPY", 5, 90.0, 50.0),
        ];

        let trades = extract_trades(&fills, &HashMap::new(), &HashMap::new());

        assert_eq!(trades.len(), 1);
        let t = &trades[0];
        assert!((t.gross_pnl - (-500.0)).abs() < 1e-10);
        assert!(!t.is_winner());
    }
}
