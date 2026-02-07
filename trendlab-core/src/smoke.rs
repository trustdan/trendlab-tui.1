//! Smoke backtest — tracer bullet proving bars flow end-to-end.
//!
//! This module is **throwaway scaffolding**. It will be replaced by the real domain model
//! (Phase 3), event loop (Phase 5b), order book (Phase 6), and execution engine (Phase 7).
//! Its only purpose is to prove the plumbing works: bars in → orders → fills → portfolio → equity.

/// Minimal bar representation for the smoke test.
#[derive(Debug, Clone)]
pub struct Bar {
    pub index: usize,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

/// Direction of a trade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Buy,
    Sell,
}

/// A pending order waiting to be filled on the next bar.
#[derive(Debug, Clone)]
pub struct PendingOrder {
    pub side: Side,
    pub quantity: u64,
}

/// A completed fill.
#[derive(Debug, Clone)]
pub struct Fill {
    pub bar_index: usize,
    pub side: Side,
    pub price: f64,
    pub quantity: u64,
}

/// A completed round-trip trade (entry + exit).
#[derive(Debug, Clone)]
pub struct Trade {
    pub entry_bar: usize,
    pub entry_price: f64,
    pub exit_bar: usize,
    pub exit_price: f64,
    pub quantity: u64,
    pub pnl: f64,
}

/// Minimal portfolio state.
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub cash: f64,
    pub shares: u64,
    pub avg_entry_price: f64,
}

impl Portfolio {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            cash: initial_capital,
            shares: 0,
            avg_entry_price: 0.0,
        }
    }

    pub fn equity(&self, current_price: f64) -> f64 {
        self.cash + self.shares as f64 * current_price
    }

    pub fn apply_fill(&mut self, fill: &Fill) {
        match fill.side {
            Side::Buy => {
                let cost = fill.price * fill.quantity as f64;
                self.cash -= cost;
                self.avg_entry_price = fill.price;
                self.shares += fill.quantity;
            }
            Side::Sell => {
                let proceeds = fill.price * fill.quantity as f64;
                self.cash += proceeds;
                self.shares -= fill.quantity;
                if self.shares == 0 {
                    self.avg_entry_price = 0.0;
                }
            }
        }
    }
}

/// Result of the smoke backtest.
#[derive(Debug)]
pub struct SmokeResult {
    pub equity_curve: Vec<f64>,
    pub trades: Vec<Trade>,
    pub fills: Vec<Fill>,
    pub final_equity: f64,
}

/// Run the smoke backtest: hardcoded buy on bar 3, sell on bar 7.
///
/// Timeline (respecting the decision/placement/fill contract):
/// - Signal to buy is generated at bar 3's close → order fills at bar 4's open
/// - Signal to sell is generated at bar 7's close → order fills at bar 8's open
pub fn run_smoke_backtest(bars: &[Bar], initial_capital: f64, quantity: u64) -> SmokeResult {
    let mut portfolio = Portfolio::new(initial_capital);
    let mut equity_curve = Vec::with_capacity(bars.len());
    let mut trades = Vec::new();
    let mut fills = Vec::new();
    let mut pending_order: Option<PendingOrder> = None;
    let mut entry_fill: Option<Fill> = None;

    for bar in bars {
        // === Start-of-bar: fill pending market orders at the open ===
        if let Some(order) = pending_order.take() {
            let fill = Fill {
                bar_index: bar.index,
                side: order.side,
                price: bar.open,
                quantity: order.quantity,
            };
            portfolio.apply_fill(&fill);

            match fill.side {
                Side::Buy => {
                    entry_fill = Some(fill.clone());
                }
                Side::Sell => {
                    if let Some(entry) = entry_fill.take() {
                        trades.push(Trade {
                            entry_bar: entry.bar_index,
                            entry_price: entry.price,
                            exit_bar: fill.bar_index,
                            exit_price: fill.price,
                            quantity: fill.quantity,
                            pnl: (fill.price - entry.price) * fill.quantity as f64,
                        });
                    }
                }
            }

            fills.push(fill);
        }

        // === End-of-bar: mark-to-market, record equity ===
        equity_curve.push(portfolio.equity(bar.close));

        // === Post-bar: generate signals (hardcoded) ===
        // Signal at bar 3 close → buy order for next bar
        if bar.index == 3 && portfolio.shares == 0 {
            pending_order = Some(PendingOrder {
                side: Side::Buy,
                quantity,
            });
        }

        // Signal at bar 7 close → sell order for next bar
        if bar.index == 7 && portfolio.shares > 0 {
            pending_order = Some(PendingOrder {
                side: Side::Sell,
                quantity,
            });
        }
    }

    let final_equity = *equity_curve.last().unwrap_or(&initial_capital);

    SmokeResult {
        equity_curve,
        trades,
        fills,
        final_equity,
    }
}

/// Create the canonical 10-bar synthetic dataset.
pub fn synthetic_bars() -> Vec<Bar> {
    vec![
        Bar {
            index: 0,
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 1000,
        },
        Bar {
            index: 1,
            open: 101.0,
            high: 103.0,
            low: 100.0,
            close: 102.0,
            volume: 1100,
        },
        Bar {
            index: 2,
            open: 102.0,
            high: 104.0,
            low: 101.0,
            close: 103.0,
            volume: 1200,
        },
        Bar {
            index: 3,
            open: 103.0,
            high: 105.0,
            low: 102.0,
            close: 104.0,
            volume: 1300,
        },
        Bar {
            index: 4,
            open: 104.0,
            high: 107.0,
            low: 103.0,
            close: 106.0,
            volume: 1400,
        },
        Bar {
            index: 5,
            open: 106.0,
            high: 108.0,
            low: 105.0,
            close: 107.0,
            volume: 1500,
        },
        Bar {
            index: 6,
            open: 107.0,
            high: 110.0,
            low: 106.0,
            close: 109.0,
            volume: 1600,
        },
        Bar {
            index: 7,
            open: 109.0,
            high: 111.0,
            low: 108.0,
            close: 110.0,
            volume: 1700,
        },
        Bar {
            index: 8,
            open: 110.0,
            high: 112.0,
            low: 109.0,
            close: 111.0,
            volume: 1800,
        },
        Bar {
            index: 9,
            open: 111.0,
            high: 113.0,
            low: 110.0,
            close: 112.0,
            volume: 1900,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden test with hand-calculated expected values.
    ///
    /// Setup:
    ///   - 10 bars, prices trending up (see `synthetic_bars`)
    ///   - Initial capital: $100,000
    ///   - Buy 100 shares at bar 4 open (104.0), sell at bar 8 open (110.0)
    ///
    /// Hand calculation:
    ///   - Bars 0-3: no position, equity = 100,000.0
    ///   - Bar 3 close: buy signal generated, order pending for bar 4
    ///   - Bar 4: fill buy at open 104.0. Cash = 100,000 - 10,400 = 89,600.
    ///            Close = 106.0 → equity = 89,600 + 100*106 = 100,200.0
    ///   - Bar 5: close = 107.0 → equity = 89,600 + 10,700 = 100,300.0
    ///   - Bar 6: close = 109.0 → equity = 89,600 + 10,900 = 100,500.0
    ///   - Bar 7: close = 110.0 → equity = 89,600 + 11,000 = 100,600.0
    ///            Sell signal generated, order pending for bar 8
    ///   - Bar 8: fill sell at open 110.0. Cash = 89,600 + 11,000 = 100,600.
    ///            Close = 111.0 → equity = 100,600.0 (flat)
    ///   - Bar 9: equity = 100,600.0 (flat)
    ///
    /// Trade PnL: (110.0 - 104.0) * 100 = 600.0
    #[test]
    fn golden_smoke_backtest() {
        let bars = synthetic_bars();
        let result = run_smoke_backtest(&bars, 100_000.0, 100);

        // Final equity
        assert_eq!(result.final_equity, 100_600.0);

        // Exact equity curve
        let expected_equity = [
            100_000.0, // bar 0: flat
            100_000.0, // bar 1: flat
            100_000.0, // bar 2: flat
            100_000.0, // bar 3: flat (signal generated at close, not yet filled)
            100_200.0, // bar 4: bought at open 104, close 106 → 89600 + 10600
            100_300.0, // bar 5: close 107 → 89600 + 10700
            100_500.0, // bar 6: close 109 → 89600 + 10900
            100_600.0, // bar 7: close 110 → 89600 + 11000 (signal generated)
            100_600.0, // bar 8: sold at open 110, cash = 100600, flat
            100_600.0, // bar 9: flat
        ];
        assert_eq!(result.equity_curve.len(), 10);
        for (i, (actual, expected)) in result
            .equity_curve
            .iter()
            .zip(expected_equity.iter())
            .enumerate()
        {
            assert_eq!(
                *actual, *expected,
                "equity mismatch at bar {i}: got {actual}, expected {expected}"
            );
        }

        // Exactly one round-trip trade
        assert_eq!(result.trades.len(), 1);
        let trade = &result.trades[0];
        assert_eq!(trade.entry_bar, 4);
        assert_eq!(trade.entry_price, 104.0);
        assert_eq!(trade.exit_bar, 8);
        assert_eq!(trade.exit_price, 110.0);
        assert_eq!(trade.quantity, 100);
        assert_eq!(trade.pnl, 600.0);

        // Exactly two fills (one buy, one sell)
        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.fills[0].side, Side::Buy);
        assert_eq!(result.fills[0].bar_index, 4);
        assert_eq!(result.fills[0].price, 104.0);
        assert_eq!(result.fills[1].side, Side::Sell);
        assert_eq!(result.fills[1].bar_index, 8);
        assert_eq!(result.fills[1].price, 110.0);
    }

    /// The smoke backtest must be deterministic — same result every run.
    #[test]
    fn smoke_backtest_is_deterministic() {
        let bars = synthetic_bars();
        let r1 = run_smoke_backtest(&bars, 100_000.0, 100);
        let r2 = run_smoke_backtest(&bars, 100_000.0, 100);

        assert_eq!(r1.final_equity, r2.final_equity);
        assert_eq!(r1.equity_curve, r2.equity_curve);
        assert_eq!(r1.trades.len(), r2.trades.len());
        assert_eq!(r1.fills.len(), r2.fills.len());
    }
}
