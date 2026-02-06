//! Level 5: Bootstrap & Regime Resampling
//!
//! The final robustness level that tests strategy stability under:
//! 1. **Block bootstrap**: Resample blocks of returns to preserve temporal structure
//! 2. **Regime resampling**: Test across different market conditions
//! 3. **Universe Monte Carlo**: Vary instrument selection
//!
//! # Key Concepts
//!
//! - **Block bootstrap**: Resample contiguous blocks (not individual days) to preserve
//!   autocorrelation and volatility clustering
//! - **Regime sensitivity**: Test how strategy performs in bull/bear/sideways markets
//! - **Universe robustness**: Ensure strategy isn't overfit to specific instruments
//!
//! # Example
//!
//! ```ignore
//! use trendlab_runner::*;
//!
//! // 500 bootstrap trials with 20-day blocks
//! let bootstrap = Bootstrap::new(
//!     500,
//!     BootstrapMode::BlockBootstrap { block_size: 20 },
//!     2.5, // min stability score (very strict)
//! );
//!
//! let result = bootstrap.run(&config)?;
//! println!("Median Sharpe: {:.2}", result.stability_score.median);
//! println!("Bootstrap CI: [{:.2}, {:.2}]",
//!     result.distributions[0].p10,
//!     result.distributions[0].p90
//! );
//! ```

use crate::config::RunConfig;
use crate::robustness::ladder::{LevelResult, RobustnessLevel};
use crate::robustness::promotion::PromotionCriteria;
use crate::robustness::stability::{MetricDistribution, StabilityScore};
use crate::runner::Runner;
use anyhow::Result;
use chrono::Duration;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

/// Bootstrap sampling strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BootstrapMode {
    /// Block bootstrap with specified block size (in days)
    BlockBootstrap { block_size: usize },
    /// Sample different date ranges (regime sensitivity)
    RegimeResampling,
    /// Randomly drop/swap instruments from universe
    UniverseMC { drop_rate: f64 },
    /// Combine all three approaches
    Mixed,
}

/// Level 5: Bootstrap validation.
///
/// # Cost
/// Extreme (N trials × 1 run = N runs per candidate)
///
/// # Purpose
/// Final stress test. Only the most robust strategies should reach this level.
/// Tests stability under:
/// - Temporal resampling (block bootstrap)
/// - Market regime variation
/// - Universe composition changes
///
/// # Promotion Criteria
/// - Very high stability score (median - penalty × IQR)
/// - Very low IQR (< 0.2 typical threshold)
/// - Sufficient trades across trials (> 30)
#[derive(Debug)]
pub struct Bootstrap {
    /// Number of bootstrap trials
    trials: usize,
    /// Bootstrap sampling strategy
    mode: BootstrapMode,
    /// Minimum stability score required
    min_stability_score: f64,
    /// Random seed for reproducibility
    seed: u64,
}

impl Bootstrap {
    /// Create a new Bootstrap level.
    ///
    /// # Arguments
    /// * `trials` - Number of bootstrap trials
    /// * `mode` - Bootstrap sampling strategy
    /// * `min_stability_score` - Minimum stability score to promote
    pub fn new(trials: usize, mode: BootstrapMode, min_stability_score: f64) -> Self {
        assert!(trials > 0, "trials must be > 0");

        Self {
            trials,
            mode,
            min_stability_score,
            seed: 42, // Fixed seed for determinism
        }
    }

    /// Run a single bootstrap trial using config resampling.
    fn run_trial(&self, base_config: &RunConfig, rng: &mut ChaCha8Rng) -> Result<(f64, usize)> {
        let config = match self.mode {
            BootstrapMode::BlockBootstrap { block_size } => {
                self.resample_blocks(base_config, block_size, rng)
            }
            BootstrapMode::RegimeResampling => self.resample_regime(base_config, rng),
            BootstrapMode::UniverseMC { drop_rate } => {
                self.resample_universe(base_config, drop_rate, rng)
            }
            BootstrapMode::Mixed => {
                // Randomly choose one of the three approaches
                let choice: f64 = rng.gen();
                if choice < 0.33 {
                    self.resample_blocks(base_config, 20, rng)
                } else if choice < 0.66 {
                    self.resample_regime(base_config, rng)
                } else {
                    self.resample_universe(base_config, 0.2, rng)
                }
            }
        };

        let runner = Runner::new();
        let result = runner.run(&config)?;

        let trade_count = result.trades.len();
        let sharpe = result.stats.sharpe;

        Ok((sharpe, trade_count))
    }

    /// Resample date range in blocks to preserve temporal structure.
    ///
    fn resample_blocks(
        &self,
        config: &RunConfig,
        _block_size: usize,
        _rng: &mut ChaCha8Rng,
    ) -> RunConfig {
        // Block bootstrap handled in `run` by resampling returns.
        config.clone()
    }

    /// Sample a random sub-period to test regime sensitivity.
    fn resample_regime(&self, config: &RunConfig, rng: &mut ChaCha8Rng) -> RunConfig {
        let total_days = (config.end_date - config.start_date).num_days();
        if total_days < 365 {
            // Too short to subsample meaningfully
            return config.clone();
        }

        // Sample a random 6-12 month window
        let window_days = rng.gen_range(180..=365);
        let max_start_offset = (total_days - window_days).max(0);
        let start_offset = rng.gen_range(0..=max_start_offset);

        let new_start = config.start_date + Duration::days(start_offset);
        let new_end = new_start + Duration::days(window_days);

        RunConfig {
            start_date: new_start,
            end_date: new_end,
            ..config.clone()
        }
    }

    /// Randomly drop or swap instruments from universe.
    fn resample_universe(
        &self,
        config: &RunConfig,
        drop_rate: f64,
        rng: &mut ChaCha8Rng,
    ) -> RunConfig {
        if config.universe.is_empty() {
            return config.clone();
        }

        let mut new_universe = config.universe.clone();

        // Drop instruments with probability drop_rate
        new_universe.retain(|_| rng.gen_bool(1.0 - drop_rate));

        // Ensure we keep at least one instrument
        if new_universe.is_empty() {
            new_universe.push(config.universe[0].clone());
        }

        RunConfig {
            universe: new_universe,
            ..config.clone()
        }
    }
}

impl RobustnessLevel for Bootstrap {
    fn name(&self) -> &str {
        "Bootstrap"
    }

    fn run(&self, config: &RunConfig) -> Result<LevelResult> {
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
        let mut sharpe_values = Vec::new();
        let mut total_trades = 0;

        match self.mode {
            BootstrapMode::BlockBootstrap { block_size } => {
                let (values, trade_count) =
                    self.run_block_bootstrap(config, block_size, &mut rng)?;
                sharpe_values = values;
                total_trades = trade_count * self.trials;
            }
            BootstrapMode::Mixed => {
                // Mixed mode: randomly dispatch each trial to the appropriate method
                for _ in 0..self.trials {
                    let choice: f64 = rng.gen();
                    if choice < 0.33 {
                        // Block bootstrap trial (single trial from block bootstrap)
                        let (values, trade_count) =
                            self.run_block_bootstrap(config, 20, &mut rng)?;
                        if let Some(v) = values.first() {
                            sharpe_values.push(*v);
                        }
                        total_trades += trade_count;
                    } else if choice < 0.66 {
                        // Regime resampling trial
                        let resampled = self.resample_regime(config, &mut rng);
                        let runner = Runner::new();
                        let result = runner.run(&resampled)?;
                        sharpe_values.push(result.stats.sharpe);
                        total_trades += result.trades.len();
                    } else {
                        // Universe MC trial
                        let resampled = self.resample_universe(config, 0.2, &mut rng);
                        let runner = Runner::new();
                        let result = runner.run(&resampled)?;
                        sharpe_values.push(result.stats.sharpe);
                        total_trades += result.trades.len();
                    }
                }
            }
            _ => {
                for _ in 0..self.trials {
                    let (sharpe, trades) = self.run_trial(config, &mut rng)?;
                    sharpe_values.push(sharpe);
                    total_trades += trades;
                }
            }
        }

        let avg_trades = if self.trials > 0 {
            total_trades / self.trials
        } else {
            0
        };
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
            max_iqr: 0.2, // Strictest requirement
            min_trades: Some(30),
            min_raw_metric: Some(self.min_stability_score + 0.2),
        }
    }
}

impl Bootstrap {
    fn run_block_bootstrap(
        &self,
        config: &RunConfig,
        block_size: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<(Vec<f64>, usize)> {
        let runner = Runner::new();
        let result = runner.run(config)?;
        let trade_count = result.trades.len();

        let mut returns = Vec::new();
        for window in result.equity_curve.windows(2) {
            if let (Some(prev), Some(next)) = (window.first(), window.get(1)) {
                if prev.equity != 0.0 {
                    returns.push((next.equity - prev.equity) / prev.equity);
                }
            }
        }

        if returns.is_empty() || block_size == 0 {
            return Ok((vec![0.0; self.trials], trade_count));
        }

        let block_size = block_size.min(returns.len());
        let mut sharpe_values = Vec::with_capacity(self.trials);

        for _ in 0..self.trials {
            let mut synthetic = Vec::with_capacity(returns.len());
            while synthetic.len() < returns.len() {
                let start = rng.gen_range(0..=returns.len() - block_size);
                let block = &returns[start..start + block_size];
                synthetic.extend_from_slice(block);
            }
            synthetic.truncate(returns.len());
            let sharpe = compute_sharpe(&synthetic);
            sharpe_values.push(sharpe);
        }

        Ok((sharpe_values, trade_count))
    }
}

fn compute_sharpe(returns: &[f64]) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();

    if std_dev > 0.0 {
        mean / std_dev * (252.0_f64).sqrt()
    } else {
        0.0
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
            end_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
            universe: vec!["SPY".to_string(), "QQQ".to_string(), "IWM".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    #[test]
    fn test_bootstrap_creation() {
        let bs = Bootstrap::new(
            500,
            BootstrapMode::BlockBootstrap { block_size: 20 },
            2.5,
        );
        assert_eq!(bs.name(), "Bootstrap");
        assert_eq!(bs.trials, 500);
    }

    #[test]
    fn test_regime_resampling() {
        let bs = Bootstrap::new(10, BootstrapMode::RegimeResampling, 2.5);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let config = mock_config();

        let resampled = bs.resample_regime(&config, &mut rng);

        // Resampled period should be within original range
        assert!(resampled.start_date >= config.start_date);
        assert!(resampled.end_date <= config.end_date);
        // Should be between 6-12 months
        let days = (resampled.end_date - resampled.start_date).num_days();
        assert!(days >= 180 && days <= 365);
    }

    #[test]
    fn test_universe_resampling() {
        let bs = Bootstrap::new(
            10,
            BootstrapMode::UniverseMC { drop_rate: 0.3 },
            2.5,
        );
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let config = mock_config();

        let mut dropped_count = 0;
        for _ in 0..100 {
            let resampled = bs.resample_universe(&config, 0.3, &mut rng);
            if resampled.universe.len() < config.universe.len() {
                dropped_count += 1;
            }
            // Should always have at least one instrument
            assert!(!resampled.universe.is_empty());
        }

        // Should drop instruments in at least some trials
        assert!(dropped_count > 0);
    }

    #[test]
    fn test_universe_resampling_empty() {
        let bs = Bootstrap::new(
            10,
            BootstrapMode::UniverseMC { drop_rate: 0.3 },
            2.5,
        );
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut config = mock_config();
        config.universe.clear();

        let resampled = bs.resample_universe(&config, 0.3, &mut rng);
        assert!(resampled.universe.is_empty());
    }

    #[test]
    fn test_bootstrap_criteria() {
        let bs = Bootstrap::new(
            500,
            BootstrapMode::BlockBootstrap { block_size: 20 },
            2.5,
        );
        let criteria = bs.promotion_criteria();

        assert_eq!(criteria.min_stability_score, 2.5);
        assert_eq!(criteria.max_iqr, 0.2);
        assert!(criteria.min_trades.is_some());
    }

    #[test]
    fn test_bootstrap_run() {
        let bs = Bootstrap::new(
            5,
            BootstrapMode::BlockBootstrap { block_size: 10 },
            2.5,
        );
        let result = bs.run(&mock_config()).unwrap();

        assert_eq!(result.level_name, "Bootstrap");
        assert_eq!(result.distributions.len(), 1);
        assert_eq!(result.distributions[0].all_values.len(), 5); // 5 trials
    }

    #[test]
    #[should_panic(expected = "trials must be > 0")]
    fn test_zero_trials_panics() {
        Bootstrap::new(
            0,
            BootstrapMode::BlockBootstrap { block_size: 20 },
            2.5,
        );
    }

    #[test]
    fn test_regime_resampling_short_period() {
        let bs = Bootstrap::new(10, BootstrapMode::RegimeResampling, 2.5);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut config = mock_config();

        // Period too short to resample
        config.end_date = config.start_date + Duration::days(100);

        let resampled = bs.resample_regime(&config, &mut rng);

        // Should return unchanged config
        assert_eq!(resampled.start_date, config.start_date);
        assert_eq!(resampled.end_date, config.end_date);
    }
}
