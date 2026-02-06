//! Backtest service trait for decoupling TUI from Runner.
//!
//! Provides a trait abstraction so the TUI can trigger reruns
//! without directly depending on the concrete Runner implementation.
//! This also enables mock implementations for testing.

use anyhow::Result;
use trendlab_runner::config::{ExecutionConfig, RunConfig};
use trendlab_runner::result::BacktestResult;

/// Abstraction for running backtests from the TUI.
pub trait BacktestService: Send + Sync {
    /// Rerun a backtest with a different execution config.
    fn rerun_with_execution(
        &self,
        base_config: &RunConfig,
        execution: &ExecutionConfig,
    ) -> Result<BacktestResult>;
}

/// Concrete implementation wrapping trendlab_runner::Runner.
pub struct RunnerService {
    runner: trendlab_runner::Runner,
}

impl Default for RunnerService {
    fn default() -> Self {
        Self::new()
    }
}

impl RunnerService {
    pub fn new() -> Self {
        Self {
            runner: trendlab_runner::Runner::new(),
        }
    }

    pub fn with_runner(runner: trendlab_runner::Runner) -> Self {
        Self { runner }
    }
}

impl BacktestService for RunnerService {
    fn rerun_with_execution(
        &self,
        base_config: &RunConfig,
        execution: &ExecutionConfig,
    ) -> Result<BacktestResult> {
        let mut config = base_config.clone();
        config.execution = execution.clone();
        self.runner.run(&config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use trendlab_runner::config::*;

    fn make_test_config() -> RunConfig {
        RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::BuyAndHold,
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2020, 6, 30).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    #[test]
    fn test_runner_service_reruns_with_different_execution() {
        let service = RunnerService::new();
        let config = make_test_config();

        let worst_case = ExecutionConfig {
            slippage: SlippageConfig::FixedBps { bps: 10.0 },
            commission: CommissionConfig::PerShare { amount: 0.005 },
            intrabar_policy: IntrabarPolicy::WorstCase,
        };

        let result = service.rerun_with_execution(&config, &worst_case).unwrap();
        assert!(!result.equity_curve.is_empty());
        // The result should have stored the modified config
        assert!(result.metadata.config.is_some());
    }

    #[test]
    fn test_different_execution_configs_produce_different_run_ids() {
        let service = RunnerService::new();
        let config = make_test_config();

        let best_case = ExecutionConfig {
            slippage: SlippageConfig::FixedBps { bps: 2.0 },
            commission: CommissionConfig::PerShare { amount: 0.005 },
            intrabar_policy: IntrabarPolicy::BestCase,
        };

        let result_base = service
            .rerun_with_execution(&config, &config.execution)
            .unwrap();
        let result_best = service
            .rerun_with_execution(&config, &best_case)
            .unwrap();

        // Different execution configs should yield different run IDs
        assert_ne!(result_base.run_id, result_best.run_id);
    }
}
