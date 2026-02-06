//! Robustness ladder for progressive strategy validation.
//!
//! This module implements a 5-level promotion ladder where cheap tests
//! filter candidates before expensive validation runs.
//!
//! # The 5-Level Promotion Ladder
//!
//! 1. **Level 1: CheapPass** - Single deterministic run with basic thresholds
//! 2. **Level 2: WalkForward** - Out-of-sample validation with train/test splits
//! 3. **Level 3: ExecutionMC** - Slippage and spread Monte Carlo
//! 4. **Level 4: PathMC** - Intrabar ambiguity Monte Carlo
//! 5. **Level 5: Bootstrap** - Block bootstrap and regime resampling
//!
//! Each level is more expensive but provides stronger robustness guarantees.
//! Candidates must pass promotion criteria at each level to advance.
//!
//! # Example: Full 5-Level Ladder
//!
//! ```ignore
//! use trendlab_runner::robustness::*;
//! use trendlab_runner::*;
//!
//! // Configure all 5 levels
//! let level1 = Box::new(CheapPass::new(
//!     1.0,  // min sharpe
//!     10,   // min trades
//! ));
//!
//! let level2 = Box::new(WalkForward::new(
//!     3,    // 3 train/test splits
//!     0.7,  // 70% train / 30% test
//!     0.8,  // min stability score
//!     10,   // min trades
//! ));
//!
//! let level3 = Box::new(ExecutionMC::new(
//!     100,  // 100 trials
//!     CostDistribution::Uniform { min: 0.0001, max: 0.0005 }, // 1-5 bps slippage
//!     1.5,  // min stability score
//! ));
//!
//! let level4 = Box::new(PathMC::new(
//!     200,  // 200 trials
//!     PathSamplingMode::Random,  // Random path ordering
//!     2.0,  // min stability score
//! ));
//!
//! let level5 = Box::new(Bootstrap::new(
//!     500,  // 500 bootstrap trials
//!     BootstrapMode::BlockBootstrap { block_size: 20 },
//!     2.5,  // min stability score (strictest)
//! ));
//!
//! // Build the ladder
//! let ladder = RobustnessLadder::new(vec![
//!     level1, level2, level3, level4, level5
//! ]);
//!
//! // Run 1000 candidates through the ladder
//! let configs: Vec<RunConfig> = generate_candidates(1000);
//! let results = ladder.run_batch(&configs)?;
//!
//! // Analyze results
//! let summary = ladder.batch_summary(&results);
//! println!("Total candidates: {}", summary.total_candidates);
//! println!("Reached Level 5: {}", summary.level_stats[4].entered);
//! println!("Final champions: {}", summary.level_stats[4].promoted);
//! println!("Compute savings: {:.1}%", summary.compute_savings * 100.0);
//! ```
//!
//! # Stability Scoring
//!
//! All levels (except CheapPass) use **stability scores** to penalize variance:
//!
//! ```text
//! stability_score = median(metric) - penalty_factor × IQR(metric)
//! ```
//!
//! This rewards consistent performance over time and across MC trials.
//!
//! # Promotion Criteria
//!
//! Each level defines:
//! - `min_stability_score`: Minimum stability score to promote
//! - `max_iqr`: Maximum interquartile range (variance penalty)
//! - `min_trades`: Minimum trade count
//! - `min_raw_metric`: Minimum median metric value
//!
//! Candidates must satisfy ALL criteria to advance.

pub mod stability;
pub mod levels;
pub mod promotion;
pub mod ladder;

pub use stability::{StabilityScore, MetricDistribution};
pub use promotion::{PromotionFilter, PromotionCriteria};
pub use ladder::{RobustnessLadder, RobustnessLevel, LevelResult};
