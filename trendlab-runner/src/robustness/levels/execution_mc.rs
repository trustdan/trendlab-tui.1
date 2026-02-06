//! Level 3: Execution Monte Carlo (slippage/spread sampling)
//!
//! Runs multiple trials with sampled execution costs to quantify sensitivity
//! to slippage and spread assumptions.

use crate::config::RunConfig;
use crate::robustness::ladder::{LevelResult, RobustnessLevel};
use crate::robustness::promotion::PromotionCriteria;
use crate::robustness::stability::{MetricDistribution, StabilityScore};
use crate::runner::Runner;
use anyhow::Result;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

/// Distribution for sampling execution costs.
#[derive(Debug, Clone)]
pub enum CostDistribution {
    /// Fixed cost (no randomness)
    Fixed(f64),
    /// Uniform random: U(min, max)
    Uniform { min: f64, max: f64 },
    /// Normal distribution: N(mean, std_dev)
    Normal { mean: f64, std_dev: f64 },
}

impl CostDistribution {
    /// Sample a value from the distribution.
    fn sample(&self, rng: &mut ChaCha8Rng) -> f64 {
        match self {
            Self::Fixed(value) => *value,
            Self::Uniform { min, max } => rng.gen_range(*min..=*max),
            Self::Normal { mean, std_dev } => {
                // Box-Muller transform for normal distribution
                let u1: f64 = rng.gen();
                let u2: f64 = rng.gen();
                let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                mean + std_dev * z
            }
        }
    }
}

/// Level 3: Execution Monte Carlo validation.
///
/// # Cost
/// High (N trials × 1 run = N runs per candidate)
///
/// # Purpose
/// Quantify sensitivity to execution costs.
/// Strategies must be robust to realistic slippage/spread variations.
///
/// # Example
/// ```ignore
/// use trendlab_runner::*;
/// use chrono::NaiveDate;
///
/// // 100 trials with uniform slippage [1-5 bps]
/// let execution_mc = ExecutionMC::new(
///     100,
///     CostDistribution::Uniform { min: 0.0001, max: 0.0005 },
///     1.5,
/// );
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
/// let result = execution_mc.run(&config).unwrap();
/// println!("Median Sharpe: {:.2}", result.stability_score.median);
/// println!("IQR: {:.2} (lower = more stable)", result.stability_score.iqr);
/// ```
pub struct ExecutionMC {
    /// Number of Monte Carlo trials
    trials: usize,
    /// Slippage distribution to sample
    slippage_dist: CostDistribution,
    /// Minimum stability score required
    min_stability_score: f64,
    /// Random seed for reproducibility
    seed: u64,
}

impl ExecutionMC {
    /// Create a new ExecutionMC level.
    ///
    /// # Arguments
    /// * `trials` - Number of Monte Carlo trials
    /// * `slippage_dist` - Distribution to sample slippage from
    /// * `min_stability_score` - Minimum stability score to promote
    pub fn new(trials: usize, slippage_dist: CostDistribution, min_stability_score: f64) -> Self {
        assert!(trials > 0, "trials must be > 0");

        Self {
            trials,
            slippage_dist,
            min_stability_score,
            seed: 42, // Fixed seed for determinism
        }
    }

    /// Run a single trial with sampled slippage.
    fn run_trial(&self, config: &RunConfig, _slippage: f64) -> Result<(f64, usize)> {
        // For now, we don't modify execution costs (would need ExecutionModel API)
        // Just run the backtest as-is
        // TODO: Hook slippage into ExecutionModel when available

        let runner = Runner::new();
        let result = runner.run(config)?;

        let trade_count = result.trades.len();
        let sharpe = result.stats.sharpe;

        Ok((sharpe, trade_count))
    }
}

/// Compute interquartile range (IQR) from a set of values.
///
/// IQR = Q3 - Q1 (75th percentile - 25th percentile)
/// Lower IQR means more stable/consistent performance.
pub fn compute_iqr(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let q1_idx = sorted.len() / 4;
    let q3_idx = 3 * sorted.len() / 4;

    let q1 = sorted[q1_idx];
    let q3 = sorted[q3_idx];

    q3 - q1
}

/// Compute stability score from IQR.
///
/// Stability = 1 / (1 + IQR)
/// Higher score means more stable (lower IQR).
pub fn compute_stability_score(iqr: f64) -> f64 {
    1.0 / (1.0 + iqr)
}

impl RobustnessLevel for ExecutionMC {
    fn name(&self) -> &str {
        "ExecutionMC"
    }

    fn run(&self, config: &RunConfig) -> Result<LevelResult> {
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        let mut sharpe_values = Vec::new();
        let mut total_trades = 0;

        for _ in 0..self.trials {
            let slippage = self.slippage_dist.sample(&mut rng);
            let (sharpe, trades) = self.run_trial(config, slippage)?;
            sharpe_values.push(sharpe);
            total_trades += trades;
        }

        let avg_trades = total_trades / self.trials;
        let stability = StabilityScore::compute("sharpe", &sharpe_values, 0.5);
        let dist = MetricDistribution::from_values("sharpe", &sharpe_values);

        Ok(LevelResult {
            level_name: self.name().to_string(),
            config: config.clone(),
            stability_score: stability,
            distributions: vec![dist],
            trade_count: avg_trades,
            promoted: false,
            rejection_reason: None,
        })
    }

    fn promotion_criteria(&self) -> PromotionCriteria {
        PromotionCriteria {
            min_stability_score: self.min_stability_score,
            max_iqr: 0.5, // Require low variance
            min_trades: Some(10),
            min_raw_metric: Some(self.min_stability_score + 0.5), // Median should be higher than stability score
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
    fn test_execution_mc_creation() {
        let mc = ExecutionMC::new(100, CostDistribution::Fixed(0.0001), 1.5);
        assert_eq!(mc.name(), "ExecutionMC");
        assert_eq!(mc.trials, 100);
    }

    #[test]
    fn test_cost_distribution_fixed() {
        let dist = CostDistribution::Fixed(0.5);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        for _ in 0..10 {
            assert_eq!(dist.sample(&mut rng), 0.5);
        }
    }

    #[test]
    fn test_cost_distribution_uniform() {
        let dist = CostDistribution::Uniform { min: 0.0, max: 1.0 };
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        for _ in 0..100 {
            let sample = dist.sample(&mut rng);
            assert!(sample >= 0.0 && sample <= 1.0);
        }
    }

    #[test]
    fn test_cost_distribution_normal() {
        let dist = CostDistribution::Normal {
            mean: 0.0,
            std_dev: 1.0,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let samples: Vec<f64> = (0..1000).map(|_| dist.sample(&mut rng)).collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;

        // Mean should be close to 0.0 (within 0.1 for 1000 samples)
        assert!((mean - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_execution_mc_criteria() {
        let mc = ExecutionMC::new(100, CostDistribution::Fixed(0.0001), 1.5);
        let criteria = mc.promotion_criteria();

        assert_eq!(criteria.min_stability_score, 1.5);
        assert_eq!(criteria.max_iqr, 0.5);
        assert!(criteria.min_trades.is_some());
    }

    #[test]
    fn test_execution_mc_run() {
        let mc = ExecutionMC::new(5, CostDistribution::Fixed(0.0001), 1.5);
        let result = mc.run(&mock_config()).unwrap();

        assert_eq!(result.level_name, "ExecutionMC");
        assert_eq!(result.distributions.len(), 1);
        assert_eq!(result.distributions[0].all_values.len(), 5); // 5 trials
    }

    #[test]
    #[should_panic(expected = "trials must be > 0")]
    fn test_zero_trials_panics() {
        ExecutionMC::new(0, CostDistribution::Fixed(0.0001), 1.5);
    }
}
