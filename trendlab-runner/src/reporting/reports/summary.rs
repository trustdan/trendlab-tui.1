//! Summary statistics for reports.

use crate::result::BacktestResult;

#[derive(Debug, Clone)]
pub struct SummaryStats {
    pub sharpe: f64,
    pub total_return: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub num_trades: usize,
}

impl SummaryStats {
    pub fn from_result(result: &BacktestResult) -> Self {
        Self {
            sharpe: result.stats.sharpe,
            total_return: result.stats.total_return,
            max_drawdown: result.stats.max_drawdown,
            win_rate: result.stats.win_rate,
            num_trades: result.stats.num_trades,
        }
    }
}
