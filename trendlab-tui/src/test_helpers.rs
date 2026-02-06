//! Test helpers for creating mock data

use trendlab_runner::result::{BacktestResult, PerformanceStats, ResultMetadata};
use std::collections::HashMap;

pub fn create_test_result(run_id: &str, sharpe: f64) -> BacktestResult {
    BacktestResult {
        run_id: run_id.to_string(),
        equity_curve: vec![],
        trades: vec![],
        stats: PerformanceStats {
            total_return: 0.45,
            annual_return: 0.15,
            sharpe,
            sortino: 3.0,
            max_drawdown: -0.12,
            calmar: 2.0,
            win_rate: 0.64,
            profit_factor: 2.3,
            num_trades: 25,
            avg_trade_return: 2.5,
            avg_win: 5.0,
            avg_loss: -2.0,
            avg_duration_days: 8.5,
            final_equity: 145000.0,
            initial_equity: 100000.0,
        },
        metadata: ResultMetadata {
            timestamp: chrono::Utc::now(),
            duration_secs: 0.1,
            custom: HashMap::new(),
            config: None,
        },
    }
}
