//! Level 2: Walk-Forward validation (train/test splits)
//!
//! Tests strategy on out-of-sample data to detect overfitting.
//! Runs multiple train/test splits in chronological order.

use crate::config::RunConfig;
use crate::robustness::ladder::{LevelResult, RobustnessLevel};
use crate::robustness::promotion::PromotionCriteria;
use crate::robustness::stability::{MetricDistribution, StabilityScore};
use crate::runner::Runner;
use anyhow::Result;
use chrono::{Duration, NaiveDate};

/// Level 2: Walk-forward out-of-sample validation.
///
/// # Cost
/// Medium (N splits × 1 run = N runs per candidate)
///
/// # Purpose
/// Detect overfitting by testing on unseen data.
/// Strategy must perform consistently across multiple OOS periods.
///
/// # Example
/// ```ignore
/// use trendlab_runner::robustness::levels::WalkForward;
/// use trendlab_runner::robustness::RobustnessLevel;
/// use trendlab_runner::*;
/// use chrono::NaiveDate;
///
/// // 5 splits with 80% train / 20% test
/// let walk_forward = WalkForward::new(5, 0.8, 0.8, 10);
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
///     start_date: NaiveDate::from_ymd_opt(2015, 1, 1).unwrap(),
///     end_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
///     universe: vec!["SPY".to_string()],
///     execution: ExecutionConfig::default(),
///     initial_capital: 100_000.0,
/// };
///
/// let result = walk_forward.run(&config).unwrap();
/// println!("OOS Median Sharpe: {:.2}", result.stability_score.median);
/// println!("OOS IQR: {:.2}", result.stability_score.iqr);
/// ```
pub struct WalkForward {
    /// Number of walk-forward splits
    num_splits: usize,
    /// Train fraction (e.g., 0.8 = 80% train, 20% test)
    train_fraction: f64,
    /// Minimum OOS Sharpe required
    min_oos_sharpe: f64,
    /// Minimum trades per split
    min_trades_per_split: usize,
}

impl WalkForward {
    /// Create a new WalkForward level.
    ///
    /// # Arguments
    /// * `num_splits` - Number of walk-forward windows
    /// * `train_fraction` - Fraction of data for training (0.0-1.0)
    /// * `min_oos_sharpe` - Minimum out-of-sample Sharpe required
    /// * `min_trades_per_split` - Minimum trades per test split
    pub fn new(
        num_splits: usize,
        train_fraction: f64,
        min_oos_sharpe: f64,
        min_trades_per_split: usize,
    ) -> Self {
        assert!(num_splits > 0, "num_splits must be > 0");
        assert!(
            (0.0..=1.0).contains(&train_fraction),
            "train_fraction must be in [0, 1]"
        );

        Self {
            num_splits,
            train_fraction,
            min_oos_sharpe,
            min_trades_per_split,
        }
    }

    /// Generate train/test date splits.
    fn generate_splits(&self, start: NaiveDate, end: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
        let total_days = (end - start).num_days();
        let split_size = total_days / self.num_splits as i64;
        let train_size = (split_size as f64 * self.train_fraction) as i64;
        let test_size = split_size - train_size;

        let mut splits = Vec::new();

        for i in 0..self.num_splits {
            let split_start = start + Duration::days(i as i64 * split_size);
            let train_end = split_start + Duration::days(train_size);
            let test_end = train_end + Duration::days(test_size);

            // Only test on OOS data (not used for training)
            splits.push((train_end, test_end));
        }

        splits
    }

    /// Run a single test split.
    fn run_split(&self, config: &RunConfig, test_start: NaiveDate, test_end: NaiveDate) -> Result<(f64, usize)> {
        let mut split_config = config.clone();
        split_config.start_date = test_start;
        split_config.end_date = test_end;

        let runner = Runner::new();
        let result = runner.run(&split_config)?;

        let trade_count = result.trades.len();
        let sharpe = result.stats.sharpe;

        Ok((sharpe, trade_count))
    }
}

impl RobustnessLevel for WalkForward {
    fn name(&self) -> &str {
        "WalkForward"
    }

    fn run(&self, config: &RunConfig) -> Result<LevelResult> {
        let splits = self.generate_splits(config.start_date, config.end_date);
        let mut oos_sharpes = Vec::new();
        let mut total_trades = 0;

        for (test_start, test_end) in splits {
            let (sharpe, trades) = self.run_split(config, test_start, test_end)?;
            oos_sharpes.push(sharpe);
            total_trades += trades;
        }

        let stability = StabilityScore::compute("oos_sharpe", &oos_sharpes, 0.5);
        let dist = MetricDistribution::from_values("oos_sharpe", &oos_sharpes);

        Ok(LevelResult {
            level_name: self.name().to_string(),
            config: config.clone(),
            stability_score: stability,
            distributions: vec![dist],
            trade_count: total_trades,
            promoted: false,
            rejection_reason: None,
        })
    }

    fn promotion_criteria(&self) -> PromotionCriteria {
        PromotionCriteria {
            min_stability_score: self.min_oos_sharpe - 0.2,
            max_iqr: 1.0, // Allow some variance across OOS periods
            min_trades: Some(self.min_trades_per_split * self.num_splits / 2), // At least half splits trade
            min_raw_metric: Some(self.min_oos_sharpe),
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
    fn test_walk_forward_creation() {
        let wf = WalkForward::new(5, 0.8, 0.8, 10);
        assert_eq!(wf.name(), "WalkForward");
        assert_eq!(wf.num_splits, 5);
        assert_eq!(wf.train_fraction, 0.8);
    }

    #[test]
    fn test_generate_splits() {
        let wf = WalkForward::new(4, 0.75, 0.8, 10);
        let start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2021, 1, 1).unwrap();

        let splits = wf.generate_splits(start, end);

        assert_eq!(splits.len(), 4);

        // Each split should have test dates
        for (test_start, test_end) in splits {
            assert!(test_start < test_end);
            assert!(test_start >= start);
            assert!(test_end <= end);
        }
    }

    #[test]
    fn test_walk_forward_criteria() {
        let wf = WalkForward::new(5, 0.8, 0.8, 10);
        let criteria = wf.promotion_criteria();

        assert!(criteria.min_stability_score < 0.8);
        assert_eq!(criteria.min_raw_metric, Some(0.8));
        assert!(criteria.min_trades.is_some());
    }

    #[test]
    fn test_walk_forward_run() {
        let wf = WalkForward::new(3, 0.8, 0.5, 5);
        let result = wf.run(&mock_config()).unwrap();

        assert_eq!(result.level_name, "WalkForward");
        assert_eq!(result.distributions.len(), 1);
        assert_eq!(result.distributions[0].all_values.len(), 3); // 3 splits
    }

    #[test]
    #[should_panic(expected = "num_splits must be > 0")]
    fn test_zero_splits_panics() {
        WalkForward::new(0, 0.8, 0.8, 10);
    }

    #[test]
    #[should_panic(expected = "train_fraction must be in [0, 1]")]
    fn test_invalid_train_fraction_panics() {
        WalkForward::new(5, 1.5, 0.8, 10);
    }
}
