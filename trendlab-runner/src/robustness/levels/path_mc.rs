//! Level 4: Path Monte Carlo (intrabar ambiguity sampling)
//!
//! When a bar can trigger multiple orders (e.g., stop-loss at low, take-profit at high),
//! the execution order is ambiguous with daily OHLC data. PathMC runs multiple trials
//! with different intrabar path assumptions to quantify this uncertainty.
//!
//! # Key Concepts
//!
//! 1. **Ambiguous bars**: Bars where price range includes multiple trigger levels
//! 2. **Path policy**: Assumption about intrabar price movement (WorstCase, BestCase, Random)
//! 3. **Path sampling**: Run multiple trials with different path policies or random orderings
//! 4. **Stability**: Strategies robust to path assumptions show low variance across trials
//!
//! # Example
//!
//! ```ignore
//! use trendlab_runner::*;
//!
//! // 200 trials with random path orderings
//! let path_mc = PathMC::new(
//!     200,
//!     PathSamplingMode::Random,
//!     2.0, // min stability score
//! );
//!
//! let result = path_mc.run(&config)?;
//! println!("Median Sharpe: {:.2}", result.stability_score.median);
//! println!("Path sensitivity: {:.2}", result.stability_score.iqr);
//! ```

use crate::config::{IntrabarPolicy, RunConfig};
use crate::robustness::ladder::{LevelResult, RobustnessLevel};
use crate::robustness::promotion::PromotionCriteria;
use crate::robustness::stability::{MetricDistribution, StabilityScore};
use crate::runner::Runner;
use anyhow::Result;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

/// How to sample intrabar paths across trials.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSamplingMode {
    /// Always use WorstCase (conservative baseline)
    WorstCase,
    /// Always use BestCase (optimistic baseline)
    BestCase,
    /// Randomly choose WorstCase or BestCase per trial
    Random,
    /// Sample from all three policies (WorstCase, BestCase, Deterministic)
    Mixed,
}

/// Level 4: Path Monte Carlo validation.
///
/// # Cost
/// Very High (N trials × 1 run = N runs per candidate)
///
/// # Purpose
/// Quantify sensitivity to intrabar ambiguity.
/// Strategies must be robust to different assumptions about execution order within bars.
///
/// # Promotion Criteria
/// - High stability score (median - penalty × IQR)
/// - Low IQR (< 0.3 typical threshold)
/// - Sufficient trades (> 20)
#[derive(Debug)]
pub struct PathMC {
    /// Number of Monte Carlo trials
    trials: usize,
    /// How to sample path policies
    sampling_mode: PathSamplingMode,
    /// Minimum stability score required
    min_stability_score: f64,
    /// Random seed for reproducibility
    seed: u64,
}

impl PathMC {
    /// Create a new PathMC level.
    ///
    /// # Arguments
    /// * `trials` - Number of Monte Carlo trials
    /// * `sampling_mode` - How to sample intrabar paths
    /// * `min_stability_score` - Minimum stability score to promote
    pub fn new(trials: usize, sampling_mode: PathSamplingMode, min_stability_score: f64) -> Self {
        assert!(trials > 0, "trials must be > 0");

        Self {
            trials,
            sampling_mode,
            min_stability_score,
            seed: 42, // Fixed seed for determinism
        }
    }

    /// Run a single trial with a specific path policy.
    ///
    /// In a real implementation, this would modify the execution model's path policy.
    /// For now, we simulate by running the same config (TODO: hook into ExecutionModel).
    fn run_trial(&self, config: &RunConfig, path_policy: IntrabarPolicy) -> Result<(f64, usize)> {
        let mut run_config = config.clone();
        run_config.execution.intrabar_policy = path_policy;

        let runner = Runner::new();
        let result = runner.run(&run_config)?;

        let trade_count = result.trades.len();
        let sharpe = result.stats.sharpe;

        Ok((sharpe, trade_count))
    }

    /// Choose path policy for a trial based on sampling mode.
    fn sample_path_policy(&self, rng: &mut ChaCha8Rng) -> IntrabarPolicy {
        match self.sampling_mode {
            PathSamplingMode::WorstCase => IntrabarPolicy::WorstCase,
            PathSamplingMode::BestCase => IntrabarPolicy::BestCase,
            PathSamplingMode::Random => {
                if rng.gen_bool(0.5) {
                    IntrabarPolicy::WorstCase
                } else {
                    IntrabarPolicy::BestCase
                }
            }
            PathSamplingMode::Mixed => {
                let choice: f64 = rng.gen();
                if choice < 0.33 {
                    IntrabarPolicy::WorstCase
                } else if choice < 0.66 {
                    IntrabarPolicy::BestCase
                } else {
                    IntrabarPolicy::OhlcOrder
                }
            }
        }
    }
}

impl RobustnessLevel for PathMC {
    fn name(&self) -> &str {
        "PathMC"
    }

    fn run(&self, config: &RunConfig) -> Result<LevelResult> {
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        let mut sharpe_values = Vec::new();
        let mut total_trades = 0;

        for _ in 0..self.trials {
            let path_policy = self.sample_path_policy(&mut rng);
            let (sharpe, trades) = self.run_trial(config, path_policy)?;
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
            max_iqr: 0.3, // Require very low variance (more strict than ExecutionMC)
            min_trades: Some(20),
            min_raw_metric: Some(self.min_stability_score + 0.3),
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
    fn test_path_mc_creation() {
        let mc = PathMC::new(200, PathSamplingMode::Random, 2.0);
        assert_eq!(mc.name(), "PathMC");
        assert_eq!(mc.trials, 200);
    }

    #[test]
    fn test_sampling_mode_worst_case() {
        let mc = PathMC::new(10, PathSamplingMode::WorstCase, 2.0);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        for _ in 0..100 {
            assert_eq!(mc.sample_path_policy(&mut rng), IntrabarPolicy::WorstCase);
        }
    }

    #[test]
    fn test_sampling_mode_best_case() {
        let mc = PathMC::new(10, PathSamplingMode::BestCase, 2.0);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        for _ in 0..100 {
            assert_eq!(mc.sample_path_policy(&mut rng), IntrabarPolicy::BestCase);
        }
    }

    #[test]
    fn test_sampling_mode_random() {
        let mc = PathMC::new(10, PathSamplingMode::Random, 2.0);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let mut worst_count = 0;
        let mut best_count = 0;

        for _ in 0..1000 {
            let policy = mc.sample_path_policy(&mut rng);
            if policy == IntrabarPolicy::WorstCase {
                worst_count += 1;
            } else if policy == IntrabarPolicy::BestCase {
                best_count += 1;
            }
        }

        // Should be roughly 50/50 (within 10% tolerance)
        assert!(worst_count > 400 && worst_count < 600);
        assert!(best_count > 400 && best_count < 600);
    }

    #[test]
    fn test_sampling_mode_mixed() {
        let mc = PathMC::new(10, PathSamplingMode::Mixed, 2.0);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let mut worst_count = 0;
        let mut best_count = 0;
        let mut det_count = 0;

        for _ in 0..1000 {
            let policy = mc.sample_path_policy(&mut rng);
            match policy {
                IntrabarPolicy::WorstCase => worst_count += 1,
                IntrabarPolicy::BestCase => best_count += 1,
                IntrabarPolicy::OhlcOrder => det_count += 1,
            }
        }

        // Each should be roughly 33% (within 10% tolerance)
        assert!(worst_count > 250 && worst_count < 400);
        assert!(best_count > 250 && best_count < 400);
        assert!(det_count > 250 && det_count < 400);
    }

    #[test]
    fn test_path_mc_criteria() {
        let mc = PathMC::new(200, PathSamplingMode::Random, 2.0);
        let criteria = mc.promotion_criteria();

        assert_eq!(criteria.min_stability_score, 2.0);
        assert_eq!(criteria.max_iqr, 0.3);
        assert!(criteria.min_trades.is_some());
    }

    #[test]
    fn test_path_mc_run() {
        let mc = PathMC::new(5, PathSamplingMode::WorstCase, 2.0);
        let result = mc.run(&mock_config()).unwrap();

        assert_eq!(result.level_name, "PathMC");
        assert_eq!(result.distributions.len(), 1);
        assert_eq!(result.distributions[0].all_values.len(), 5); // 5 trials
    }

    #[test]
    #[should_panic(expected = "trials must be > 0")]
    fn test_zero_trials_panics() {
        PathMC::new(0, PathSamplingMode::Random, 2.0);
    }
}
