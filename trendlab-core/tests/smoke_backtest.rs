//! M0.5 Smoke backtest integration test
//!
//! This is a golden test with hardcoded buy/sell logic.
//! Purpose: validate the tracer-bullet integration path works end-to-end.

use chrono::Utc;
use trendlab_core::domain::Bar;
use trendlab_core::engine::smoke::SmokeEngine;

#[test]
fn smoke_backtest_produces_golden_equity() {
    // Load synthetic 10-bar dataset
    let bars = load_synthetic_bars();

    // Hardcoded strategy: buy bar 3, sell bar 7
    let mut engine = SmokeEngine::new(10000.0);

    for (i, bar) in bars.iter().enumerate() {
        if i == 3 {
            engine.execute_buy(bar, i, 100.0); // buy $100 worth
        }
        if i == 7 {
            engine.execute_sell(bar, i);
        }
        engine.mark_to_market(bar);
    }

    let final_equity = engine.equity();

    // Golden value: calculated manually
    // Entry: $100 @ 110.0 = 0.909 shares
    // Exit: 0.909 shares @ 120.0 = $109.09
    // Profit: $9.09
    // Final equity: $10000 - $100 + $109.09 = $10009.09
    assert!(
        (final_equity - 10009.09).abs() < 0.1,
        "Golden equity mismatch: expected ~10009.09, got {}",
        final_equity
    );

    let trades = engine.trades();
    assert_eq!(trades.len(), 1, "Expected exactly 1 round-trip trade");
    assert!((trades[0].pnl - 9.09).abs() < 0.1, "Expected ~$9.09 profit");

    // Print visual confirmation
    println!("\n✓ Smoke backtest PASSED");
    println!("  Final equity: ${:.2}", final_equity);
    println!("  Trades: {}", trades.len());
    println!(
        "  [0] Entry: bar {} @ ${:.2}, Exit: bar {} @ ${:.2}, PnL: ${:.2}",
        trades[0].entry_bar,
        trades[0].entry_price,
        trades[0].exit_bar,
        trades[0].exit_price,
        trades[0].pnl
    );
}

fn load_synthetic_bars() -> Vec<Bar> {
    let base_time = Utc::now();
    // 10 bars with predictable price movement
    // Entry at bar 3 (close=110), exit at bar 7 (close=120) = +$10/share
    vec![
        Bar::new(base_time, "TEST".into(), 100.0, 105.0, 95.0, 100.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 100.0, 110.0, 98.0, 105.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 105.0, 108.0, 102.0, 107.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 107.0, 112.0, 106.0, 110.0, 1000.0), // BUY HERE
        Bar::new(base_time, "TEST".into(), 110.0, 115.0, 108.0, 112.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 112.0, 118.0, 111.0, 115.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 115.0, 120.0, 114.0, 118.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 118.0, 125.0, 117.0, 120.0, 1000.0), // SELL HERE (+$10/share)
        Bar::new(base_time, "TEST".into(), 120.0, 122.0, 118.0, 119.0, 1000.0),
        Bar::new(base_time, "TEST".into(), 119.0, 121.0, 117.0, 120.0, 1000.0),
    ]
}
