//! Backtest result and performance statistics.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::RunId;

/// Complete result of a backtest run.
///
/// Contains:
/// - Equity curve (daily portfolio values)
/// - Trade log
/// - Performance statistics
/// - Configuration hash for caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    /// Unique identifier for the run configuration
    pub run_id: RunId,

    /// Daily equity curve: date -> portfolio value
    pub equity_curve: Vec<EquityPoint>,

    /// Trade log
    pub trades: Vec<TradeRecord>,

    /// Performance statistics
    pub stats: PerformanceStats,

    /// Metadata
    pub metadata: ResultMetadata,
}

/// Single point in the equity curve.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EquityPoint {
    pub date: NaiveDate,
    pub equity: f64,
}

/// Trade record (simplified from trendlab-core's FilledOrder).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeRecord {
    pub symbol: String,
    pub entry_date: NaiveDate,
    pub exit_date: NaiveDate,
    pub direction: TradeDirection,
    pub entry_price: f64,
    pub exit_price: f64,
    pub quantity: i64,
    pub pnl: f64,
    pub return_pct: f64,
    /// Signal that triggered this trade (e.g. "Long", "Short")
    #[serde(default)]
    pub signal_intent: Option<String>,
    /// Order type used (e.g. "Market(MOO)", "StopMarket(99.5)")
    #[serde(default)]
    pub order_type: Option<String>,
    /// Fill context (e.g. "Filled at open $105.23")
    #[serde(default)]
    pub fill_context: Option<String>,
    /// Entry slippage in dollars (from FillResult)
    #[serde(default)]
    pub entry_slippage: Option<f64>,
    /// Exit slippage in dollars (from FillResult)
    #[serde(default)]
    pub exit_slippage: Option<f64>,
    /// Whether entry fill was gapped through (from FillResult)
    #[serde(default)]
    pub entry_was_gapped: Option<bool>,
    /// Whether exit fill was gapped through (from FillResult)
    #[serde(default)]
    pub exit_was_gapped: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TradeDirection {
    Long,
    Short,
}

/// Performance statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    /// Total return (fractional, e.g., 0.25 = 25%)
    pub total_return: f64,

    /// Annualized return
    pub annual_return: f64,

    /// Sharpe ratio (assuming risk-free rate = 0)
    pub sharpe: f64,

    /// Sortino ratio (downside deviation only)
    pub sortino: f64,

    /// Maximum drawdown (fractional)
    pub max_drawdown: f64,

    /// Calmar ratio (annual return / max drawdown)
    pub calmar: f64,

    /// Win rate (fraction of profitable trades)
    pub win_rate: f64,

    /// Profit factor (gross profit / gross loss)
    pub profit_factor: f64,

    /// Total number of trades
    pub num_trades: usize,

    /// Average trade return (%)
    pub avg_trade_return: f64,

    /// Average winning trade return (%)
    pub avg_win: f64,

    /// Average losing trade return (%)
    pub avg_loss: f64,

    /// Average trade duration in days
    pub avg_duration_days: f64,

    /// Final equity
    pub final_equity: f64,

    /// Initial equity
    pub initial_equity: f64,
}

impl PerformanceStats {
    /// Computes statistics from equity curve and trade log.
    pub fn from_results(
        equity_curve: &[EquityPoint],
        trades: &[TradeRecord],
        initial_capital: f64,
    ) -> Self {
        if equity_curve.is_empty() {
            return Self::default();
        }

        let final_equity = equity_curve.last().unwrap().equity;
        let total_return = (final_equity - initial_capital) / initial_capital;

        // Compute number of years
        let first_date = equity_curve.first().unwrap().date;
        let last_date = equity_curve.last().unwrap().date;
        let days = (last_date - first_date).num_days() as f64;
        let years = days / 365.25;
        let annual_return = if years > 0.0 {
            (1.0 + total_return).powf(1.0 / years) - 1.0
        } else {
            0.0
        };

        // Compute daily returns
        let daily_returns: Vec<f64> = equity_curve
            .windows(2)
            .map(|w| (w[1].equity - w[0].equity) / w[0].equity)
            .collect();

        let sharpe = compute_sharpe(&daily_returns);
        let sortino = compute_sortino(&daily_returns);
        let max_drawdown = compute_max_drawdown(equity_curve);
        let calmar = if max_drawdown.abs() > 1e-9 {
            annual_return / max_drawdown
        } else {
            0.0
        };

        // Trade statistics
        let num_trades = trades.len();
        let winning_trades: Vec<_> = trades.iter().filter(|t| t.pnl > 0.0).collect();
        let losing_trades: Vec<_> = trades.iter().filter(|t| t.pnl <= 0.0).collect();

        let win_rate = if num_trades > 0 {
            winning_trades.len() as f64 / num_trades as f64
        } else {
            0.0
        };

        let gross_profit: f64 = winning_trades.iter().map(|t| t.pnl).sum();
        let gross_loss: f64 = losing_trades.iter().map(|t| t.pnl.abs()).sum();
        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else {
            0.0
        };

        let avg_trade_return = if num_trades > 0 {
            trades.iter().map(|t| t.return_pct).sum::<f64>() / num_trades as f64
        } else {
            0.0
        };

        let avg_win = if !winning_trades.is_empty() {
            winning_trades.iter().map(|t| t.return_pct).sum::<f64>() / winning_trades.len() as f64
        } else {
            0.0
        };

        let avg_loss = if !losing_trades.is_empty() {
            losing_trades.iter().map(|t| t.return_pct).sum::<f64>() / losing_trades.len() as f64
        } else {
            0.0
        };

        let avg_duration_days = if num_trades > 0 {
            trades
                .iter()
                .map(|t| (t.exit_date - t.entry_date).num_days() as f64)
                .sum::<f64>()
                / num_trades as f64
        } else {
            0.0
        };

        Self {
            total_return,
            annual_return,
            sharpe,
            sortino,
            max_drawdown,
            calmar,
            win_rate,
            profit_factor,
            num_trades,
            avg_trade_return,
            avg_win,
            avg_loss,
            avg_duration_days,
            final_equity,
            initial_equity: initial_capital,
        }
    }

    /// Returns a fitness score for this result (used by leaderboard).
    ///
    /// Default fitness: Sharpe ratio.
    pub fn fitness(&self) -> f64 {
        self.sharpe
    }
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            total_return: 0.0,
            annual_return: 0.0,
            sharpe: 0.0,
            sortino: 0.0,
            max_drawdown: 0.0,
            calmar: 0.0,
            win_rate: 0.0,
            profit_factor: 0.0,
            num_trades: 0,
            avg_trade_return: 0.0,
            avg_win: 0.0,
            avg_loss: 0.0,
            avg_duration_days: 0.0,
            final_equity: 0.0,
            initial_equity: 0.0,
        }
    }
}

/// Metadata about the backtest run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultMetadata {
    /// When the backtest was run
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// How long it took to run (seconds)
    pub duration_secs: f64,

    /// Additional custom metadata
    pub custom: HashMap<String, serde_json::Value>,

    /// The config that produced this result (for reruns and manifest viewing)
    #[serde(default)]
    pub config: Option<crate::config::RunConfig>,
}

// Helper functions for statistics

fn compute_sharpe(daily_returns: &[f64]) -> f64 {
    if daily_returns.is_empty() {
        return 0.0;
    }

    let mean = daily_returns.iter().sum::<f64>() / daily_returns.len() as f64;
    let variance = daily_returns
        .iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>()
        / daily_returns.len() as f64;
    let std_dev = variance.sqrt();

    if std_dev > 0.0 {
        // Annualize: sqrt(252 trading days)
        mean / std_dev * (252.0_f64).sqrt()
    } else {
        0.0
    }
}

fn compute_sortino(daily_returns: &[f64]) -> f64 {
    if daily_returns.is_empty() {
        return 0.0;
    }

    let mean = daily_returns.iter().sum::<f64>() / daily_returns.len() as f64;

    // Downside deviation: only consider negative returns
    let downside_returns: Vec<f64> = daily_returns.iter().filter(|&&r| r < 0.0).copied().collect();

    if downside_returns.is_empty() {
        return 0.0;
    }

    let downside_variance = downside_returns
        .iter()
        .map(|r| r.powi(2))
        .sum::<f64>()
        / downside_returns.len() as f64;
    let downside_dev = downside_variance.sqrt();

    if downside_dev > 0.0 {
        // Annualize
        mean / downside_dev * (252.0_f64).sqrt()
    } else {
        0.0
    }
}

fn compute_max_drawdown(equity_curve: &[EquityPoint]) -> f64 {
    if equity_curve.is_empty() {
        return 0.0;
    }

    let mut peak = equity_curve[0].equity;
    let mut max_dd = 0.0;

    for point in equity_curve {
        if point.equity > peak {
            peak = point.equity;
        }
        let dd = (peak - point.equity) / peak;
        if dd > max_dd {
            max_dd = dd;
        }
    }

    max_dd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharpe_calculation() {
        let returns = vec![0.01, 0.02, -0.01, 0.03, 0.00, 0.01];
        let sharpe = compute_sharpe(&returns);
        assert!(sharpe > 0.0, "Positive mean return should yield positive Sharpe");
    }

    #[test]
    fn test_max_drawdown_calculation() {
        let equity_curve = vec![
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
                equity: 100_000.0,
            },
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 2).unwrap(),
                equity: 110_000.0,
            },
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 3).unwrap(),
                equity: 90_000.0, // 18.2% drawdown from peak
            },
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 4).unwrap(),
                equity: 95_000.0,
            },
        ];

        let max_dd = compute_max_drawdown(&equity_curve);
        assert!((max_dd - 0.1818).abs() < 0.001, "Max DD should be ~18.18%");
    }

    #[test]
    fn test_performance_stats_from_results() {
        let equity_curve = vec![
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
                equity: 100_000.0,
            },
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 6, 30).unwrap(),
                equity: 110_000.0,
            },
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 12, 31).unwrap(),
                equity: 120_000.0,
            },
        ];

        let trades = vec![TradeRecord {
            symbol: "SPY".to_string(),
            entry_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            exit_date: NaiveDate::from_ymd_opt(2020, 6, 30).unwrap(),
            direction: TradeDirection::Long,
            entry_price: 300.0,
            exit_price: 330.0,
            quantity: 100,
            pnl: 3000.0,
            return_pct: 10.0,
            signal_intent: Some("Long".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $300.00".to_string()),
            entry_slippage: Some(0.15),
            exit_slippage: Some(0.10),
            entry_was_gapped: Some(false),
            exit_was_gapped: Some(false),
        }];

        let stats = PerformanceStats::from_results(&equity_curve, &trades, 100_000.0);

        assert_eq!(stats.total_return, 0.2);
        assert_eq!(stats.num_trades, 1);
        assert_eq!(stats.win_rate, 1.0);
    }
}
