//! Portfolio update — applies fills to the portfolio.
//!
//! Handles position creation, position closure, realized PnL calculation,
//! and cash accounting after fills.

use crate::domain::instrument::OrderSide;
use crate::domain::position::{Position, PositionSide};
use crate::domain::{Fill, Portfolio};

/// Apply a batch of fills to the portfolio.
///
/// For each fill:
/// - Buy fill: deduct net cost from cash, create or add to position
/// - Sell fill: add net proceeds to cash, reduce or close position, compute realized PnL
///
/// The equity accounting identity (`equity == cash + positions`) must hold
/// after every call.
pub fn apply_fills(fills: &[Fill], portfolio: &mut Portfolio) {
    for fill in fills {
        match fill.side {
            OrderSide::Buy => apply_buy_fill(fill, portfolio),
            OrderSide::Sell => apply_sell_fill(fill, portfolio),
        }
        portfolio.total_commission += fill.commission;
        portfolio.total_slippage += fill.slippage;
    }
}

/// Apply a buy fill: deduct cost from cash, create or add to position.
fn apply_buy_fill(fill: &Fill, portfolio: &mut Portfolio) {
    let cost = fill.net_amount(); // gross + commission + slippage
    portfolio.cash -= cost;

    if let Some(pos) = portfolio.positions.get_mut(&fill.symbol) {
        if pos.side == PositionSide::Short {
            // Covering a short position (reducing)
            let covered_qty = fill.quantity.min(pos.quantity);
            let realized = (pos.avg_entry_price - fill.price) * covered_qty;
            pos.realized_pnl += realized;
            pos.quantity -= covered_qty;

            if pos.quantity <= 1e-10 {
                pos.side = PositionSide::Flat;
                pos.quantity = 0.0;
            }
        } else if pos.side == PositionSide::Long {
            // Adding to a long position (averaging in)
            let total_cost = pos.avg_entry_price * pos.quantity + fill.price * fill.quantity;
            let total_qty = pos.quantity + fill.quantity;
            pos.avg_entry_price = total_cost / total_qty;
            pos.quantity = total_qty;
        } else {
            // Flat → open new long
            *pos = Position::new_long(
                fill.symbol.clone(),
                fill.quantity,
                fill.price,
                fill.bar_index,
            );
        }
    } else {
        // New long position
        portfolio.positions.insert(
            fill.symbol.clone(),
            Position::new_long(
                fill.symbol.clone(),
                fill.quantity,
                fill.price,
                fill.bar_index,
            ),
        );
    }
}

/// Apply a sell fill: add proceeds to cash, reduce or close position.
fn apply_sell_fill(fill: &Fill, portfolio: &mut Portfolio) {
    let proceeds = fill.net_amount(); // gross - commission - slippage
    portfolio.cash += proceeds;

    if let Some(pos) = portfolio.positions.get_mut(&fill.symbol) {
        if pos.side == PositionSide::Long {
            // Selling from a long position (reducing)
            let sold_qty = fill.quantity.min(pos.quantity);
            let realized = (fill.price - pos.avg_entry_price) * sold_qty;
            pos.realized_pnl += realized;
            pos.quantity -= sold_qty;

            if pos.quantity <= 1e-10 {
                pos.side = PositionSide::Flat;
                pos.quantity = 0.0;
            }
        } else if pos.side == PositionSide::Short {
            // Adding to a short position
            let total_cost = pos.avg_entry_price * pos.quantity + fill.price * fill.quantity;
            let total_qty = pos.quantity + fill.quantity;
            pos.avg_entry_price = total_cost / total_qty;
            pos.quantity = total_qty;
        } else {
            // Flat → open new short
            *pos = Position::new_short(
                fill.symbol.clone(),
                fill.quantity,
                fill.price,
                fill.bar_index,
            );
        }
    } else {
        // New short position
        portfolio.positions.insert(
            fill.symbol.clone(),
            Position::new_short(
                fill.symbol.clone(),
                fill.quantity,
                fill.price,
                fill.bar_index,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fill::FillPhase;
    use crate::domain::ids::OrderId;
    use chrono::NaiveDate;

    fn buy_fill(symbol: &str, price: f64, qty: f64) -> Fill {
        Fill {
            order_id: OrderId(1),
            bar_index: 0,
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            symbol: symbol.into(),
            side: OrderSide::Buy,
            price,
            quantity: qty,
            commission: 0.0,
            slippage: 0.0,
            phase: FillPhase::StartOfBar,
        }
    }

    fn sell_fill(symbol: &str, price: f64, qty: f64) -> Fill {
        Fill {
            order_id: OrderId(2),
            bar_index: 5,
            date: NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(),
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
    fn buy_creates_long_position() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[buy_fill("SPY", 100.0, 50.0)], &mut portfolio);

        assert_eq!(portfolio.cash, 95_000.0); // 100k - 100*50
        let pos = portfolio.get_position("SPY").unwrap();
        assert_eq!(pos.side, PositionSide::Long);
        assert_eq!(pos.quantity, 50.0);
        assert_eq!(pos.avg_entry_price, 100.0);
    }

    #[test]
    fn sell_closes_long_position() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[buy_fill("SPY", 100.0, 50.0)], &mut portfolio);
        apply_fills(&[sell_fill("SPY", 110.0, 50.0)], &mut portfolio);

        // Cash: 95000 + 110*50 = 100500
        assert!((portfolio.cash - 100_500.0).abs() < 1e-10);
        // Position should be flat
        assert!(!portfolio.has_position("SPY"));
    }

    #[test]
    fn partial_sell_reduces_position() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[buy_fill("SPY", 100.0, 100.0)], &mut portfolio);
        apply_fills(&[sell_fill("SPY", 110.0, 30.0)], &mut portfolio);

        let pos = portfolio.get_position("SPY").unwrap();
        assert_eq!(pos.quantity, 70.0);
        assert_eq!(pos.side, PositionSide::Long);
    }

    #[test]
    fn realized_pnl_on_close() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[buy_fill("SPY", 100.0, 50.0)], &mut portfolio);
        apply_fills(&[sell_fill("SPY", 110.0, 50.0)], &mut portfolio);

        let pos = portfolio.positions.get("SPY").unwrap();
        // Realized PnL: (110 - 100) * 50 = 500
        assert!((pos.realized_pnl - 500.0).abs() < 1e-10);
    }

    #[test]
    fn buy_averages_into_existing_long() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[buy_fill("SPY", 100.0, 50.0)], &mut portfolio);
        apply_fills(&[buy_fill("SPY", 110.0, 50.0)], &mut portfolio);

        let pos = portfolio.get_position("SPY").unwrap();
        assert_eq!(pos.quantity, 100.0);
        // Avg price: (100*50 + 110*50) / 100 = 105
        assert!((pos.avg_entry_price - 105.0).abs() < 1e-10);
    }

    #[test]
    fn sell_creates_short_position() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[sell_fill("SPY", 100.0, 50.0)], &mut portfolio);

        // Cash: 100k + 100*50 = 105k
        assert!((portfolio.cash - 105_000.0).abs() < 1e-10);
        let pos = portfolio.get_position("SPY").unwrap();
        assert_eq!(pos.side, PositionSide::Short);
        assert_eq!(pos.quantity, 50.0);
    }

    #[test]
    fn buy_covers_short_position() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[sell_fill("SPY", 100.0, 50.0)], &mut portfolio);
        apply_fills(&[buy_fill("SPY", 90.0, 50.0)], &mut portfolio);

        // Cash: 100k + 100*50 - 90*50 = 100500
        assert!((portfolio.cash - 100_500.0).abs() < 1e-10);
        assert!(!portfolio.has_position("SPY"));
    }

    #[test]
    fn commission_and_slippage_tracked() {
        let mut portfolio = Portfolio::new(100_000.0);
        let mut fill = buy_fill("SPY", 100.0, 50.0);
        fill.commission = 5.0;
        fill.slippage = 2.0;
        apply_fills(&[fill], &mut portfolio);

        assert_eq!(portfolio.total_commission, 5.0);
        assert_eq!(portfolio.total_slippage, 2.0);
        // Cash deducted includes commission and slippage: 100*50 + 5 + 2 = 5007
        assert!((portfolio.cash - (100_000.0 - 5_007.0)).abs() < 1e-10);
    }

    #[test]
    fn equity_identity_after_round_trip() {
        let mut portfolio = Portfolio::new(100_000.0);
        apply_fills(&[buy_fill("SPY", 100.0, 100.0)], &mut portfolio);

        // After buy: cash=90000, position=100 shares at current price
        let mut prices = std::collections::HashMap::new();
        prices.insert("SPY".into(), 105.0);
        let equity = portfolio.equity(&prices);
        // 90000 + 100*105 = 100500
        assert!((equity - 100_500.0).abs() < 1e-10);

        apply_fills(&[sell_fill("SPY", 105.0, 100.0)], &mut portfolio);
        let prices_empty = std::collections::HashMap::new();
        let equity_after = portfolio.equity(&prices_empty);
        // All cash now: 90000 + 105*100 = 100500
        assert!((equity_after - 100_500.0).abs() < 1e-10);
    }
}
