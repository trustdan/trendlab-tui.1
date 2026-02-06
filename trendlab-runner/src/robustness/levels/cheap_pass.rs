//! Level 1: Cheap Pass (deterministic baseline)
//!
//! Fast, single-run validation with deterministic execution.
//! Purpose: Filter obvious losers before expensive validation.

use crate::config::RunConfig;
use crate::robustness::ladder::{LevelResult, RobustnessLevel};
use crate::robustness::promotion::PromotionCriteria;
use crate::robustness::stability::{MetricDistribution, StabilityScore};
use crate::runner::Runner;
use anyhow::Result;

/// Level 1: Deterministic baseline run.
///
/// # Cost
/// Very low (single run per candidate)
///
/// # Purpose
/// Filter strategies with obvious flaws:
/// - Negative Sharpe
/// - Too few trades
/// - Unrealistic returns
///
/// # Example
/// ```ignore
/// use trendlab_runner::robustness::levels::CheapPass;
/// use trendlab_runner::robustness::RobustnessLevel;
/// use trendlab_runner::*;
/// use chrono::NaiveDate;
///
/// let cheap_pass = CheapPass::new(0.5, 5);
///
/// let config = RunConfig {
///     strategy: StrategyConfig {
///         signal_generator: SignalGeneratorConfig::MaCrossover {
///             short_period: 10,
///             long_period: 50,
///         },
///         order_policy: OrderPolicyConfig::Simple,
///         position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
///     },
///     start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
///     end_date: NaiveDate::from_ymd_opt(2021, 1, 1).unwrap(),
///     universe: vec!["SPY".to_string()],
///     execution: ExecutionConfig::default(),
///     initial_capital: 100_000.0,
/// };
///
/// let result = cheap_pass.run(&config).unwrap();
/// println!("Sharpe: {:.2}", result.stability_score.median);
/// ```
pub struct CheapPass {
    /// Minimum Sharpe ratio to pass
    min_sharpe: f64,
    /// Minimum number of trades required
    min_trades: usize,
}

impl CheapPass {
    /// Create a new CheapPass level.
    pub fn new(min_sharpe: f64, min_trades: usize) -> Self {
        Self {
            min_sharpe,
            min_trades,
        }
    }

    /// Run a single deterministic backtest.
    fn run_single(&self, config: &RunConfig) -> Result<(f64, usize)> {
        let runner = Runner::new();
        let result = runner.run(config)?;

        let trade_count = result.trades.len();
        let sharpe = result.stats.sharpe;

        Ok((sharpe, trade_count))
    }
}

impl RobustnessLevel for CheapPass {
    fn name(&self) -> &str {
        "CheapPass"
    }

    fn run(&self, config: &RunConfig) -> Result<LevelResult> {
        let (sharpe, trade_count) = self.run_single(config)?;

        // For deterministic run, "distribution" is a single value
        let sharpe_values = vec![sharpe];
        let stability = StabilityScore::compute("sharpe", &sharpe_values, 0.5);
        let dist = MetricDistribution::from_values("sharpe", &sharpe_values);

        Ok(LevelResult {
            level_name: self.name().to_string(),
            config: config.clone(),
            stability_score: stability,
            distributions: vec![dist],
            trade_count,
            promoted: false,
            rejection_reason: None,
        })
    }

    fn promotion_criteria(&self) -> PromotionCriteria {
        PromotionCriteria {
            min_stability_score: self.min_sharpe - 0.1, // Slightly below raw threshold
            max_iqr: f64::INFINITY, // No variance check for single run
            min_trades: Some(self.min_trades),
            min_raw_metric: Some(self.min_sharpe),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, SignalGeneratorConfig,
        StrategyConfig,
    };
    use chrono::NaiveDate;

    fn mock_config() -> RunConfig {
        RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::MaCrossover {
                    short_period: 10,
                    long_period: 50,
                },
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2021, 1, 1).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    #[test]
    fn test_cheap_pass_creation() {
        let cheap_pass = CheapPass::new(1.0, 10);
        assert_eq!(cheap_pass.name(), "CheapPass");
        assert_eq!(cheap_pass.min_sharpe, 1.0);
        assert_eq!(cheap_pass.min_trades, 10);
    }

    #[test]
    fn test_cheap_pass_criteria() {
        let cheap_pass = CheapPass::new(1.0, 10);
        let criteria = cheap_pass.promotion_criteria();

        assert_eq!(criteria.min_raw_metric, Some(1.0));
        assert_eq!(criteria.min_trades, Some(10));
        assert!(criteria.max_iqr.is_infinite());
    }

    #[test]
    fn test_cheap_pass_run() {
        let cheap_pass = CheapPass::new(0.5, 5);
        let result = cheap_pass.run(&mock_config()).unwrap();

        assert_eq!(result.level_name, "CheapPass");
        assert_eq!(result.distributions.len(), 1);
        assert_eq!(result.distributions[0].all_values.len(), 1); // Single run
    }
}
