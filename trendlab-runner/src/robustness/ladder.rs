//! Robustness ladder orchestrator.
//!
//! Progressive filtering: cheap tests → expensive validation
//! Only candidates that pass cheap levels get promoted to expensive ones.

use serde::{Deserialize, Serialize};
use super::promotion::{PromotionCriteria, PromotionFilter};
use super::stability::{MetricDistribution, StabilityScore};
use crate::config::RunConfig;
use anyhow::Result;

/// Result from a single robustness level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelResult {
    /// Level name (e.g., "CheapPass", "WalkForward")
    pub level_name: String,
    /// Strategy configuration
    pub config: RunConfig,
    /// Stability score for primary metric (e.g., Sharpe)
    pub stability_score: StabilityScore,
    /// Full metric distributions
    pub distributions: Vec<MetricDistribution>,
    /// Number of trades executed
    pub trade_count: usize,
    /// Whether this candidate promoted to next level
    pub promoted: bool,
    /// Rejection reason if not promoted
    pub rejection_reason: Option<String>,
}

/// Trait for robustness levels in the promotion ladder.
pub trait RobustnessLevel: Send + Sync {
    /// Level name (for logging/reporting)
    fn name(&self) -> &str;

    /// Run validation for a strategy configuration.
    ///
    /// Returns a LevelResult with stability scores and distributions.
    fn run(&self, config: &RunConfig) -> Result<LevelResult>;

    /// Get promotion criteria for this level.
    fn promotion_criteria(&self) -> PromotionCriteria;
}

/// Orchestrates progressive filtering through multiple robustness levels.
///
/// # Design
/// - Cheap tests filter many candidates quickly
/// - Expensive tests run only on promoted subset
/// - Saves 90%+ of compute budget compared to running MC on all candidates
///
/// # Example Flow
/// ```text
/// 1000 candidates → Level 1 (cheap)    → 100 promoted
/// 100 candidates  → Level 2 (medium)   → 20 promoted
/// 20 candidates   → Level 3 (expensive) → 5 promoted
/// ```
pub struct RobustnessLadder {
    levels: Vec<Box<dyn RobustnessLevel>>,
}

impl RobustnessLadder {
    /// Create a new ladder with the given levels.
    pub fn new(levels: Vec<Box<dyn RobustnessLevel>>) -> Self {
        Self { levels }
    }

    /// Run a single candidate through all levels until it fails or completes.
    ///
    /// Returns results for each level the candidate reached.
    pub fn run_candidate(&self, config: &RunConfig) -> Result<Vec<LevelResult>> {
        let mut results = Vec::new();

        for level in &self.levels {
            let mut result = level.run(config)?;
            let criteria = level.promotion_criteria();
            let filter = PromotionFilter::new(criteria);

            // Check if candidate promotes
            let should_promote = filter.should_promote(
                &result.stability_score,
                result.trade_count,
                result.stability_score.median,
            );

            result.promoted = should_promote;

            if !should_promote {
                result.rejection_reason = Some(
                    filter
                        .rejection_reason(
                            &result.stability_score,
                            result.trade_count,
                            result.stability_score.median,
                        )
                        .unwrap_or_else(|| "Failed promotion criteria".to_string()),
                );
                results.push(result);
                break; // Stop progression
            }

            results.push(result);
        }

        Ok(results)
    }

    /// Run multiple candidates through the ladder in parallel.
    ///
    /// Returns all results, grouped by level.
    pub fn run_batch(&self, configs: &[RunConfig]) -> Result<Vec<Vec<LevelResult>>> {
        use rayon::prelude::*;

        let all_results: Vec<_> = configs
            .par_iter()
            .map(|config| self.run_candidate(config))
            .collect::<Result<Vec<_>>>()?;

        Ok(all_results)
    }

    /// Get summary statistics for a batch run.
    pub fn batch_summary(&self, results: &[Vec<LevelResult>]) -> LadderSummary {
        let mut summary = LadderSummary {
            total_candidates: results.len(),
            level_stats: Vec::new(),
        };

        if self.levels.is_empty() {
            return summary;
        }

        // Count promotions at each level
        for (level_idx, level) in self.levels.iter().enumerate() {
            let entered = results
                .iter()
                .filter(|r| r.len() > level_idx)
                .count();

            let promoted = results
                .iter()
                .filter(|r| r.len() > level_idx && r[level_idx].promoted)
                .count();

            let rejected = entered - promoted;
            let rejection_rate = if entered > 0 {
                (rejected as f64 / entered as f64) * 100.0
            } else {
                0.0
            };

            summary.level_stats.push(LevelStats {
                level_name: level.name().to_string(),
                entered,
                promoted,
                rejected,
                rejection_rate,
            });
        }

        summary
    }
}

/// Summary statistics for a batch run through the ladder.
#[derive(Debug, Clone)]
pub struct LadderSummary {
    pub total_candidates: usize,
    pub level_stats: Vec<LevelStats>,
}

/// Statistics for a single level in the ladder.
#[derive(Debug, Clone)]
pub struct LevelStats {
    pub level_name: String,
    pub entered: usize,
    pub promoted: usize,
    pub rejected: usize,
    pub rejection_rate: f64,
}

impl LadderSummary {
    /// Print a human-readable summary.
    pub fn display(&self) {
        println!("=== Robustness Ladder Summary ===");
        println!("Total candidates: {}", self.total_candidates);
        println!();

        for stats in &self.level_stats {
            println!("Level: {}", stats.level_name);
            println!("  Entered:   {}", stats.entered);
            println!("  Promoted:  {}", stats.promoted);
            println!("  Rejected:  {} ({:.1}%)", stats.rejected, stats.rejection_rate);
            println!();
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

    // Mock robustness level for testing
    struct MockLevel {
        name: String,
        sharpe_values: Vec<f64>,
        trade_count: usize,
        criteria: PromotionCriteria,
    }

    impl RobustnessLevel for MockLevel {
        fn name(&self) -> &str {
            &self.name
        }

        fn run(&self, config: &RunConfig) -> Result<LevelResult> {
            let stability = StabilityScore::compute("sharpe", &self.sharpe_values, 0.5);
            let dist = MetricDistribution::from_values("sharpe", &self.sharpe_values);

            Ok(LevelResult {
                level_name: self.name.clone(),
                config: config.clone(),
                stability_score: stability,
                distributions: vec![dist],
                trade_count: self.trade_count,
                promoted: false,
                rejection_reason: None,
            })
        }

        fn promotion_criteria(&self) -> PromotionCriteria {
            self.criteria.clone()
        }
    }

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
    fn test_single_candidate_promotes() {
        let level1 = Box::new(MockLevel {
            name: "Level1".to_string(),
            sharpe_values: vec![2.0, 2.1, 1.9], // Stable, high
            trade_count: 20,
            criteria: PromotionCriteria {
                min_stability_score: 1.0,
                max_iqr: 1.0,
                min_trades: Some(10),
                min_raw_metric: Some(1.0),
            },
        });

        let ladder = RobustnessLadder::new(vec![level1]);
        let results = ladder.run_candidate(&mock_config()).unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].promoted);
        assert!(results[0].rejection_reason.is_none());
    }

    #[test]
    fn test_single_candidate_rejects() {
        let level1 = Box::new(MockLevel {
            name: "Level1".to_string(),
            sharpe_values: vec![0.5, 0.6, 0.4], // Low sharpe
            trade_count: 20,
            criteria: PromotionCriteria {
                min_stability_score: 1.0,
                max_iqr: 1.0,
                min_trades: Some(10),
                min_raw_metric: Some(1.0),
            },
        });

        let ladder = RobustnessLadder::new(vec![level1]);
        let results = ladder.run_candidate(&mock_config()).unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].promoted);
        assert!(results[0].rejection_reason.is_some());
    }

    #[test]
    fn test_two_level_progression() {
        let level1 = Box::new(MockLevel {
            name: "Level1".to_string(),
            sharpe_values: vec![2.0, 2.1, 1.9],
            trade_count: 20,
            criteria: PromotionCriteria {
                min_stability_score: 1.0,
                max_iqr: 1.0,
                min_trades: Some(10),
                min_raw_metric: Some(1.0),
            },
        });

        let level2 = Box::new(MockLevel {
            name: "Level2".to_string(),
            sharpe_values: vec![1.8, 1.9, 2.0],
            trade_count: 20,
            criteria: PromotionCriteria {
                min_stability_score: 1.5,
                max_iqr: 0.5,
                min_trades: Some(10),
                min_raw_metric: Some(1.5),
            },
        });

        let ladder = RobustnessLadder::new(vec![level1, level2]);
        let results = ladder.run_candidate(&mock_config()).unwrap();

        assert_eq!(results.len(), 2); // Made it through both levels
        assert!(results[0].promoted);
        assert!(results[1].promoted);
    }

    #[test]
    fn test_stops_at_failed_level() {
        let level1 = Box::new(MockLevel {
            name: "Level1".to_string(),
            sharpe_values: vec![0.5, 0.6, 0.4], // Fails
            trade_count: 20,
            criteria: PromotionCriteria {
                min_stability_score: 1.0,
                max_iqr: 1.0,
                min_trades: Some(10),
                min_raw_metric: Some(1.0),
            },
        });

        let level2 = Box::new(MockLevel {
            name: "Level2".to_string(),
            sharpe_values: vec![2.0, 2.1, 2.0],
            trade_count: 20,
            criteria: PromotionCriteria::default_for_level(2),
        });

        let ladder = RobustnessLadder::new(vec![level1, level2]);
        let results = ladder.run_candidate(&mock_config()).unwrap();

        assert_eq!(results.len(), 1); // Stopped at level 1
        assert!(!results[0].promoted);
    }
}
