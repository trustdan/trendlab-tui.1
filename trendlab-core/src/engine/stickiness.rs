//! Stickiness diagnostics — measures how "sticky" a strategy's positions are.
//!
//! Stickiness is the primary failure mode of trend-following backtests: positions
//! that never exit because the stop keeps chasing the price. These metrics
//! quantify the problem and flag pathological configurations.

use crate::domain::TradeRecord;

/// Stickiness metrics computed for a backtest run.
#[derive(Debug, Clone)]
pub struct StickinessMetrics {
    /// Median holding period in bars.
    pub median_holding_bars: f64,
    /// 95th percentile holding period in bars.
    pub p95_holding_bars: f64,
    /// Fraction of trades held longer than 60 bars (~3 months).
    pub pct_over_60_bars: f64,
    /// Fraction of trades held longer than 120 bars (~6 months).
    pub pct_over_120_bars: f64,
    /// Exit trigger rate: fraction of PM calls that returned non-Hold.
    /// Low rate = sticky (the PM keeps holding instead of adjusting).
    pub exit_trigger_rate: f64,
    /// Inverse of exit trigger rate (capped at 100.0).
    /// High ratio = the exit reference keeps running away from price.
    pub reference_chase_ratio: f64,
}

/// Compute stickiness metrics from completed trades and PM call counters.
///
/// Returns None if there are no completed trades.
pub fn compute_stickiness(
    trades: &[TradeRecord],
    pm_calls_total: usize,
    pm_calls_active: usize,
) -> Option<StickinessMetrics> {
    if trades.is_empty() {
        return None;
    }

    let mut holding_periods: Vec<f64> = trades.iter().map(|t| t.bars_held as f64).collect();
    holding_periods.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let n = holding_periods.len();
    let median_holding_bars = percentile(&holding_periods, 50.0);
    let p95_holding_bars = percentile(&holding_periods, 95.0);

    let over_60 = holding_periods.iter().filter(|&&h| h > 60.0).count();
    let over_120 = holding_periods.iter().filter(|&&h| h > 120.0).count();
    let pct_over_60_bars = over_60 as f64 / n as f64;
    let pct_over_120_bars = over_120 as f64 / n as f64;

    let exit_trigger_rate = if pm_calls_total > 0 {
        pm_calls_active as f64 / pm_calls_total as f64
    } else {
        0.0
    };

    let reference_chase_ratio = if exit_trigger_rate > 0.0 {
        (1.0 / exit_trigger_rate).min(100.0)
    } else {
        100.0 // max cap when exit never triggers
    };

    Some(StickinessMetrics {
        median_holding_bars,
        p95_holding_bars,
        pct_over_60_bars,
        pct_over_120_bars,
        exit_trigger_rate,
        reference_chase_ratio,
    })
}

/// Compute the p-th percentile of a sorted slice using linear interpolation.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return sorted[0];
    }
    let rank = (p / 100.0) * (n - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let frac = rank - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_trade(bars_held: usize) -> TradeRecord {
        let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        TradeRecord {
            symbol: "SPY".into(),
            side: crate::domain::PositionSide::Long,
            entry_bar: 0,
            entry_date: date,
            entry_price: 100.0,
            exit_bar: bars_held,
            exit_date: date,
            exit_price: 105.0,
            quantity: 100.0,
            gross_pnl: 500.0,
            commission: 0.0,
            slippage: 0.0,
            net_pnl: 500.0,
            bars_held,
            mae: -50.0,
            mfe: 600.0,
            signal_id: None,
            signal_type: None,
            pm_type: None,
            execution_model: None,
            filter_type: None,
        }
    }

    #[test]
    fn no_trades_returns_none() {
        assert!(compute_stickiness(&[], 0, 0).is_none());
    }

    #[test]
    fn single_trade() {
        let trades = vec![make_trade(20)];
        let m = compute_stickiness(&trades, 100, 50).unwrap();
        assert!((m.median_holding_bars - 20.0).abs() < 1e-10);
        assert!((m.p95_holding_bars - 20.0).abs() < 1e-10);
        assert!((m.pct_over_60_bars - 0.0).abs() < 1e-10);
        assert!((m.exit_trigger_rate - 0.5).abs() < 1e-10);
        assert!((m.reference_chase_ratio - 2.0).abs() < 1e-10);
    }

    #[test]
    fn multiple_trades_median() {
        let trades = vec![
            make_trade(10),
            make_trade(20),
            make_trade(30),
            make_trade(40),
            make_trade(50),
        ];
        let m = compute_stickiness(&trades, 200, 100).unwrap();
        assert!((m.median_holding_bars - 30.0).abs() < 1e-10);
    }

    #[test]
    fn over_60_and_120_thresholds() {
        let trades = vec![
            make_trade(10),
            make_trade(50),
            make_trade(70),  // > 60
            make_trade(100), // > 60
            make_trade(130), // > 60 and > 120
        ];
        let m = compute_stickiness(&trades, 100, 50).unwrap();
        assert!((m.pct_over_60_bars - 0.6).abs() < 1e-10); // 3/5
        assert!((m.pct_over_120_bars - 0.2).abs() < 1e-10); // 1/5
    }

    #[test]
    fn zero_pm_calls() {
        let trades = vec![make_trade(10)];
        let m = compute_stickiness(&trades, 0, 0).unwrap();
        assert!((m.exit_trigger_rate - 0.0).abs() < 1e-10);
        assert!((m.reference_chase_ratio - 100.0).abs() < 1e-10);
    }

    #[test]
    fn chase_ratio_capped() {
        let trades = vec![make_trade(10)];
        // Very low exit trigger rate: 1 / 10000
        let m = compute_stickiness(&trades, 10000, 1).unwrap();
        assert!((m.reference_chase_ratio - 100.0).abs() < 1e-10);
    }
}
