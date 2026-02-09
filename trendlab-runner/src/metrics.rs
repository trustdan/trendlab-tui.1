//! Performance metrics — pure functions that compute strategy statistics.
//!
//! Every metric is a pure function: equity curve and/or trade list in, scalar out.
//! No dependencies on the runner, data pipeline, or engine.

use serde::{Deserialize, Serialize};
use trendlab_core::domain::TradeRecord;

/// Aggregate performance metrics for a single backtest run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub cagr: f64,
    pub sharpe: f64,
    pub sortino: f64,
    pub calmar: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub trade_count: usize,
    pub turnover: f64,
    pub max_consecutive_wins: usize,
    pub max_consecutive_losses: usize,
    pub avg_losing_streak: f64,
}

impl PerformanceMetrics {
    /// Compute all metrics from an equity curve and trade list.
    pub fn compute(equity_curve: &[f64], trades: &[TradeRecord], initial_capital: f64) -> Self {
        let trading_days = equity_curve.len();
        Self {
            total_return: total_return(equity_curve),
            cagr: cagr(equity_curve, trading_days),
            sharpe: sharpe_ratio(equity_curve, 0.0),
            sortino: sortino_ratio(equity_curve, 0.0),
            calmar: calmar_ratio(equity_curve, trading_days),
            max_drawdown: max_drawdown(equity_curve),
            win_rate: win_rate(trades),
            profit_factor: profit_factor(trades),
            trade_count: trades.len(),
            turnover: turnover(trades, initial_capital, trading_days),
            max_consecutive_wins: max_consecutive_wins(trades),
            max_consecutive_losses: max_consecutive_losses(trades),
            avg_losing_streak: avg_losing_streak(trades),
        }
    }
}

// ─── Individual metric functions ────────────────────────────────────

/// Total return as a fraction: (final - initial) / initial.
pub fn total_return(equity_curve: &[f64]) -> f64 {
    if equity_curve.len() < 2 {
        return 0.0;
    }
    let initial = equity_curve[0];
    let final_eq = *equity_curve.last().unwrap();
    if initial <= 0.0 {
        return 0.0;
    }
    (final_eq - initial) / initial
}

/// Compound Annual Growth Rate.
///
/// Assumes 252 trading days per year. Returns 0.0 for single-bar or constant equity.
pub fn cagr(equity_curve: &[f64], trading_days: usize) -> f64 {
    if equity_curve.len() < 2 || trading_days < 2 {
        return 0.0;
    }
    let initial = equity_curve[0];
    let final_eq = *equity_curve.last().unwrap();
    if initial <= 0.0 || final_eq <= 0.0 {
        return 0.0;
    }
    let years = trading_days as f64 / 252.0;
    if years <= 0.0 {
        return 0.0;
    }
    (final_eq / initial).powf(1.0 / years) - 1.0
}

/// Annualized Sharpe ratio from daily returns.
///
/// Sharpe = mean(daily returns - rf) / std(daily returns) * sqrt(252).
/// Returns 0.0 if variance is zero or fewer than 2 bars.
pub fn sharpe_ratio(equity_curve: &[f64], risk_free_rate: f64) -> f64 {
    let returns = daily_returns(equity_curve);
    if returns.len() < 2 {
        return 0.0;
    }
    let daily_rf = risk_free_rate / 252.0;
    let excess: Vec<f64> = returns.iter().map(|r| r - daily_rf).collect();
    let mean = mean_f64(&excess);
    let std = std_dev(&excess);
    if std < 1e-15 {
        return 0.0;
    }
    (mean / std) * (252.0_f64).sqrt()
}

/// Annualized Sortino ratio (downside deviation only).
///
/// Sortino = mean(daily returns - rf) / downside_std * sqrt(252).
/// Returns 0.0 if no downside deviation or fewer than 2 bars.
pub fn sortino_ratio(equity_curve: &[f64], risk_free_rate: f64) -> f64 {
    let returns = daily_returns(equity_curve);
    if returns.len() < 2 {
        return 0.0;
    }
    let daily_rf = risk_free_rate / 252.0;
    let excess: Vec<f64> = returns.iter().map(|r| r - daily_rf).collect();
    let mean = mean_f64(&excess);

    // Downside deviation: std of only negative excess returns
    let downside_sq: Vec<f64> = excess.iter().filter(|&&r| r < 0.0).map(|r| r * r).collect();

    if downside_sq.is_empty() {
        return 0.0; // No downside → ratio undefined
    }

    let downside_var = downside_sq.iter().sum::<f64>() / returns.len() as f64;
    let downside_std = downside_var.sqrt();
    if downside_std < 1e-15 {
        return 0.0;
    }
    (mean / downside_std) * (252.0_f64).sqrt()
}

/// Calmar ratio: CAGR / |max_drawdown|.
///
/// Returns 0.0 if max drawdown is zero or CAGR is non-positive.
pub fn calmar_ratio(equity_curve: &[f64], trading_days: usize) -> f64 {
    let c = cagr(equity_curve, trading_days);
    let dd = max_drawdown(equity_curve);
    if dd >= 0.0 || c <= 0.0 {
        return 0.0;
    }
    c / dd.abs()
}

/// Maximum drawdown as a negative fraction (e.g., -0.15 = 15% drawdown).
///
/// Returns 0.0 if equity is constant or monotonically increasing.
pub fn max_drawdown(equity_curve: &[f64]) -> f64 {
    if equity_curve.len() < 2 {
        return 0.0;
    }
    let mut peak = equity_curve[0];
    let mut max_dd = 0.0_f64;

    for &eq in equity_curve {
        if eq > peak {
            peak = eq;
        }
        if peak > 0.0 {
            let dd = (eq - peak) / peak;
            if dd < max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

/// Win rate: fraction of trades that were winners.
pub fn win_rate(trades: &[TradeRecord]) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }
    let winners = trades.iter().filter(|t| t.is_winner()).count();
    winners as f64 / trades.len() as f64
}

/// Profit factor: gross profits / gross losses.
///
/// Capped at 100.0 for edge cases (all winners, zero losses).
pub fn profit_factor(trades: &[TradeRecord]) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }
    let gross_profit: f64 = trades
        .iter()
        .filter(|t| t.net_pnl > 0.0)
        .map(|t| t.net_pnl)
        .sum();
    let gross_loss: f64 = trades
        .iter()
        .filter(|t| t.net_pnl < 0.0)
        .map(|t| t.net_pnl.abs())
        .sum();

    if gross_loss < 1e-10 {
        return if gross_profit > 0.0 { 100.0 } else { 0.0 };
    }
    (gross_profit / gross_loss).min(100.0)
}

/// Annual turnover: total traded notional / average capital / years.
pub fn turnover(trades: &[TradeRecord], initial_capital: f64, trading_days: usize) -> f64 {
    if trades.is_empty() || initial_capital <= 0.0 || trading_days < 2 {
        return 0.0;
    }
    let total_notional: f64 = trades
        .iter()
        .map(|t| t.entry_price * t.quantity + t.exit_price * t.quantity)
        .sum();
    let years = trading_days as f64 / 252.0;
    if years <= 0.0 {
        return 0.0;
    }
    total_notional / initial_capital / years
}

/// Maximum consecutive winning trades.
pub fn max_consecutive_wins(trades: &[TradeRecord]) -> usize {
    max_consecutive(trades, true)
}

/// Maximum consecutive losing trades.
pub fn max_consecutive_losses(trades: &[TradeRecord]) -> usize {
    max_consecutive(trades, false)
}

/// Average length of losing streaks.
pub fn avg_losing_streak(trades: &[TradeRecord]) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }
    let mut streaks: Vec<usize> = Vec::new();
    let mut current = 0;

    for trade in trades {
        if !trade.is_winner() {
            current += 1;
        } else {
            if current > 0 {
                streaks.push(current);
            }
            current = 0;
        }
    }
    if current > 0 {
        streaks.push(current);
    }

    if streaks.is_empty() {
        return 0.0;
    }
    streaks.iter().sum::<usize>() as f64 / streaks.len() as f64
}

// ─── Helpers ────────────────────────────────────────────────────────

/// Compute daily returns from an equity curve.
pub fn daily_returns(equity_curve: &[f64]) -> Vec<f64> {
    if equity_curve.len() < 2 {
        return Vec::new();
    }
    equity_curve
        .windows(2)
        .map(|w| {
            if w[0] > 0.0 {
                (w[1] - w[0]) / w[0]
            } else {
                0.0
            }
        })
        .collect()
}

pub(crate) fn mean_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

pub(crate) fn std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = mean_f64(values);
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

fn max_consecutive(trades: &[TradeRecord], winners: bool) -> usize {
    let mut max_streak = 0;
    let mut current = 0;

    for trade in trades {
        if trade.is_winner() == winners {
            current += 1;
            if current > max_streak {
                max_streak = current;
            }
        } else {
            current = 0;
        }
    }
    max_streak
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use trendlab_core::domain::position::PositionSide;

    fn make_trade(net_pnl: f64) -> TradeRecord {
        let date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        TradeRecord {
            symbol: "SPY".into(),
            side: PositionSide::Long,
            entry_bar: 0,
            entry_date: date,
            entry_price: 100.0,
            exit_bar: 5,
            exit_date: date,
            exit_price: if net_pnl >= 0.0 {
                100.0 + net_pnl / 50.0
            } else {
                100.0 + net_pnl / 50.0
            },
            quantity: 50.0,
            gross_pnl: net_pnl,
            commission: 0.0,
            slippage: 0.0,
            net_pnl,
            bars_held: 5,
            mae: 0.0,
            mfe: 0.0,
            signal_id: None,
            signal_type: None,
            pm_type: None,
            execution_model: None,
            filter_type: None,
        }
    }

    // ── Total return ──

    #[test]
    fn total_return_positive() {
        let eq = vec![100_000.0, 100_500.0, 101_000.0, 110_000.0];
        assert!((total_return(&eq) - 0.1).abs() < 1e-10);
    }

    #[test]
    fn total_return_negative() {
        let eq = vec![100_000.0, 95_000.0, 90_000.0];
        assert!((total_return(&eq) - (-0.1)).abs() < 1e-10);
    }

    #[test]
    fn total_return_constant() {
        let eq = vec![100_000.0, 100_000.0, 100_000.0];
        assert!((total_return(&eq) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn total_return_single_bar() {
        assert_eq!(total_return(&[100_000.0]), 0.0);
    }

    #[test]
    fn total_return_empty() {
        assert_eq!(total_return(&[]), 0.0);
    }

    // ── CAGR ──

    #[test]
    fn cagr_one_year() {
        // 252 bars, 10% total return → CAGR ≈ 10%
        let mut eq = vec![100_000.0];
        for i in 1..252 {
            let daily_r = (1.1_f64).powf(1.0 / 251.0);
            eq.push(eq[i - 1] * daily_r);
        }
        let c = cagr(&eq, 252);
        assert!((c - 0.1).abs() < 0.005, "CAGR should be ~10%, got {c}");
    }

    #[test]
    fn cagr_constant_equity() {
        let eq = vec![100_000.0; 252];
        assert_eq!(cagr(&eq, 252), 0.0);
    }

    #[test]
    fn cagr_single_bar() {
        assert_eq!(cagr(&[100_000.0], 1), 0.0);
    }

    // ── Sharpe ──

    #[test]
    fn sharpe_constant_equity_is_zero() {
        let eq = vec![100_000.0; 100];
        assert_eq!(sharpe_ratio(&eq, 0.0), 0.0);
    }

    #[test]
    fn sharpe_known_returns() {
        // Alternating daily gains: +0.2%, +0.05% → positive mean, small std
        let mut eq = vec![100_000.0];
        for i in 1..253 {
            let r = if i % 2 == 0 { 1.002 } else { 1.0005 };
            eq.push(eq[i - 1] * r);
        }
        let s = sharpe_ratio(&eq, 0.0);
        // Both days are positive returns, mean ≈ 0.125%, std small → high Sharpe
        assert!(
            s > 5.0,
            "Sharpe should be high for consistently positive returns, got {s}"
        );
    }

    #[test]
    fn sharpe_constant_return_is_zero() {
        // Perfectly constant daily return → zero std → Sharpe = 0
        let mut eq = vec![100_000.0];
        for i in 1..253 {
            eq.push(eq[i - 1] * 1.001);
        }
        assert_eq!(sharpe_ratio(&eq, 0.0), 0.0);
    }

    #[test]
    fn sharpe_single_bar() {
        assert_eq!(sharpe_ratio(&[100_000.0], 0.0), 0.0);
    }

    // ── Sortino ──

    #[test]
    fn sortino_no_downside_is_zero() {
        // Monotonically increasing equity
        let eq: Vec<f64> = (0..100).map(|i| 100_000.0 + i as f64 * 100.0).collect();
        assert_eq!(sortino_ratio(&eq, 0.0), 0.0);
    }

    #[test]
    fn sortino_with_downside() {
        // Create an equity curve with some down days
        let mut eq = vec![100_000.0];
        for _ in 0..50 {
            eq.push(*eq.last().unwrap() * 1.002);
        }
        for _ in 0..10 {
            eq.push(*eq.last().unwrap() * 0.995);
        }
        for _ in 0..50 {
            eq.push(*eq.last().unwrap() * 1.002);
        }
        let s = sortino_ratio(&eq, 0.0);
        assert!(s > 0.0, "Sortino should be positive, got {s}");
    }

    // ── Max drawdown ──

    #[test]
    fn max_drawdown_known() {
        let eq = vec![100_000.0, 110_000.0, 90_000.0, 95_000.0];
        // Peak = 110k, trough = 90k → dd = (90k-110k)/110k = -18.18%
        let dd = max_drawdown(&eq);
        let expected = (90_000.0 - 110_000.0) / 110_000.0;
        assert!((dd - expected).abs() < 1e-10);
    }

    #[test]
    fn max_drawdown_monotonic_increase() {
        let eq: Vec<f64> = (0..100).map(|i| 100_000.0 + i as f64 * 100.0).collect();
        assert_eq!(max_drawdown(&eq), 0.0);
    }

    #[test]
    fn max_drawdown_constant() {
        let eq = vec![100_000.0; 100];
        assert_eq!(max_drawdown(&eq), 0.0);
    }

    #[test]
    fn max_drawdown_empty() {
        assert_eq!(max_drawdown(&[]), 0.0);
    }

    // ── Win rate ──

    #[test]
    fn win_rate_all_winners() {
        let trades = vec![make_trade(500.0), make_trade(300.0)];
        assert!((win_rate(&trades) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn win_rate_all_losers() {
        let trades = vec![make_trade(-500.0), make_trade(-300.0)];
        assert!((win_rate(&trades) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn win_rate_mixed() {
        let trades = vec![
            make_trade(500.0),
            make_trade(-200.0),
            make_trade(300.0),
            make_trade(-100.0),
        ];
        assert!((win_rate(&trades) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn win_rate_empty() {
        assert_eq!(win_rate(&[]), 0.0);
    }

    // ── Profit factor ──

    #[test]
    fn profit_factor_mixed() {
        let trades = vec![make_trade(500.0), make_trade(-200.0), make_trade(300.0)];
        // Profit = 800, Loss = 200 → PF = 4.0
        assert!((profit_factor(&trades) - 4.0).abs() < 1e-10);
    }

    #[test]
    fn profit_factor_all_winners_capped() {
        let trades = vec![make_trade(500.0), make_trade(300.0)];
        assert!((profit_factor(&trades) - 100.0).abs() < 1e-10);
    }

    #[test]
    fn profit_factor_all_losers() {
        let trades = vec![make_trade(-500.0), make_trade(-300.0)];
        assert!((profit_factor(&trades) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn profit_factor_empty() {
        assert_eq!(profit_factor(&[]), 0.0);
    }

    // ── Calmar ──

    #[test]
    fn calmar_positive_cagr_with_drawdown() {
        let mut eq = vec![100_000.0];
        // Go up, then dip, then recover higher
        for _ in 0..126 {
            eq.push(*eq.last().unwrap() * 1.001);
        }
        for _ in 0..30 {
            eq.push(*eq.last().unwrap() * 0.998);
        }
        for _ in 0..96 {
            eq.push(*eq.last().unwrap() * 1.002);
        }
        let c = calmar_ratio(&eq, eq.len());
        assert!(c > 0.0, "Calmar should be positive, got {c}");
    }

    #[test]
    fn calmar_no_drawdown_is_zero() {
        let eq: Vec<f64> = (0..252).map(|i| 100_000.0 + i as f64 * 100.0).collect();
        assert_eq!(calmar_ratio(&eq, 252), 0.0);
    }

    // ── Consecutive wins/losses ──

    #[test]
    fn consecutive_wins() {
        let trades = vec![
            make_trade(100.0),  // W
            make_trade(200.0),  // W
            make_trade(300.0),  // W
            make_trade(-100.0), // L
            make_trade(200.0),  // W
        ];
        assert_eq!(max_consecutive_wins(&trades), 3);
    }

    #[test]
    fn consecutive_losses() {
        let trades = vec![
            make_trade(100.0),  // W
            make_trade(-200.0), // L
            make_trade(-300.0), // L
            make_trade(-100.0), // L
            make_trade(200.0),  // W
        ];
        assert_eq!(max_consecutive_losses(&trades), 3);
    }

    #[test]
    fn consecutive_empty() {
        assert_eq!(max_consecutive_wins(&[]), 0);
        assert_eq!(max_consecutive_losses(&[]), 0);
    }

    // ── Average losing streak ──

    #[test]
    fn avg_losing_streak_mixed() {
        let trades = vec![
            make_trade(-100.0), // L streak 1
            make_trade(-200.0),
            make_trade(300.0),  // W
            make_trade(-100.0), // L streak 2
            make_trade(200.0),  // W
        ];
        // Two streaks: [2, 1], avg = 1.5
        assert!((avg_losing_streak(&trades) - 1.5).abs() < 1e-10);
    }

    #[test]
    fn avg_losing_streak_no_losses() {
        let trades = vec![make_trade(100.0), make_trade(200.0)];
        assert_eq!(avg_losing_streak(&trades), 0.0);
    }

    #[test]
    fn avg_losing_streak_empty() {
        assert_eq!(avg_losing_streak(&[]), 0.0);
    }

    // ── Turnover ──

    #[test]
    fn turnover_basic() {
        let trades = vec![make_trade(500.0)]; // entry=100, exit~=110, qty=50
                                              // Total notional = 100*50 + 110*50 = 10500
                                              // initial_capital = 100k, years = 252/252 = 1
        let t = turnover(&trades, 100_000.0, 252);
        assert!(t > 0.0);
    }

    #[test]
    fn turnover_empty() {
        assert_eq!(turnover(&[], 100_000.0, 252), 0.0);
    }

    // ── Aggregate ──

    #[test]
    fn compute_all_metrics_no_trades() {
        let eq = vec![100_000.0; 100];
        let m = PerformanceMetrics::compute(&eq, &[], 100_000.0);
        assert_eq!(m.total_return, 0.0);
        assert_eq!(m.trade_count, 0);
        assert_eq!(m.win_rate, 0.0);
        assert_eq!(m.sharpe, 0.0);
        assert!(m.total_return.is_finite());
        assert!(m.sharpe.is_finite());
        assert!(m.sortino.is_finite());
    }

    #[test]
    fn compute_all_metrics_with_trades() {
        // Use alternating returns to get non-zero Sharpe
        let mut eq = vec![100_000.0];
        for i in 1..253 {
            let r = if i % 2 == 0 { 1.001 } else { 1.0003 };
            eq.push(eq[i - 1] * r);
        }
        let trades = vec![make_trade(500.0), make_trade(-200.0), make_trade(300.0)];
        let m = PerformanceMetrics::compute(&eq, &trades, 100_000.0);
        assert!(m.total_return > 0.0);
        assert!(m.sharpe > 0.0);
        assert_eq!(m.trade_count, 3);
        assert!((m.win_rate - 2.0 / 3.0).abs() < 1e-10);
        // All metrics should be finite
        assert!(m.total_return.is_finite());
        assert!(m.cagr.is_finite());
        assert!(m.sharpe.is_finite());
        assert!(m.sortino.is_finite());
        assert!(m.calmar.is_finite());
        assert!(m.max_drawdown.is_finite());
        assert!(m.profit_factor.is_finite());
        assert!(m.turnover.is_finite());
        assert!(m.avg_losing_streak.is_finite());
    }

    // ── Daily returns helper ──

    #[test]
    fn daily_returns_basic() {
        let eq = vec![100.0, 110.0, 105.0];
        let r = daily_returns(&eq);
        assert_eq!(r.len(), 2);
        assert!((r[0] - 0.1).abs() < 1e-10);
        let expected = (105.0 - 110.0) / 110.0;
        assert!((r[1] - expected).abs() < 1e-10);
    }
}
