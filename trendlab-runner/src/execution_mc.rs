//! Execution Monte Carlo — sensitivity analysis for slippage, commission, and
//! path policy.
//!
//! Samples execution parameters from uniform distributions, runs backtests with
//! each sample, and computes a stability score that rewards high median performance
//! with low variance.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use trendlab_core::components::execution::{GapPolicy, PathPolicy};
use trendlab_core::data::align::AlignedData;
use trendlab_core::engine::execution::CostModel;
use trendlab_core::engine::ExecutionConfig;
use trendlab_core::fingerprint::{StrategyConfig, TradingMode};

use crate::runner::{run_backtest_with_exec_config, RunError};

// ─── Configuration ───────────────────────────────────────────────────

/// Configuration for execution Monte Carlo sampling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMcConfig {
    /// Number of MC samples (default 20).
    pub n_samples: usize,
    /// Slippage range in basis points: (min, max). Uniform sampling.
    pub slippage_range: (f64, f64),
    /// Commission range in basis points: (min, max). Uniform sampling.
    pub commission_range: (f64, f64),
    /// Path policies to sample from.
    pub path_policies: Vec<PathPolicy>,
    /// RNG seed for reproducibility.
    pub seed: u64,
}

impl Default for ExecutionMcConfig {
    fn default() -> Self {
        Self {
            n_samples: 20,
            slippage_range: (0.0, 30.0),
            commission_range: (0.0, 20.0),
            path_policies: vec![PathPolicy::Deterministic, PathPolicy::WorstCase, PathPolicy::BestCase],
            seed: 42,
        }
    }
}

// ─── Result types ────────────────────────────────────────────────────

/// A single MC sample: execution parameters and resulting metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McSample {
    pub slippage_bps: f64,
    pub commission_bps: f64,
    pub path_policy: PathPolicy,
    pub sharpe: f64,
    pub cagr: f64,
    pub max_drawdown: f64,
    pub trade_count: usize,
}

/// Stability score: summarizes the distribution of outcomes across MC samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityScore {
    /// Median Sharpe across all MC samples.
    pub median_sharpe: f64,
    /// Interquartile range (P75 - P25) of Sharpe.
    pub iqr_sharpe: f64,
    /// 10th percentile Sharpe (pessimistic estimate).
    pub p10_sharpe: f64,
    /// Stability ratio: median / (1 + IQR). Higher = more stable.
    pub stability_ratio: f64,
    /// Sanity check: true if not all samples are identical.
    pub all_different: bool,
}

/// Complete result of execution MC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMcResult {
    pub samples: Vec<McSample>,
    pub stability: StabilityScore,
}

/// Errors from execution MC.
#[derive(Debug, Error)]
pub enum McError {
    #[error("backtest failed for MC sample {sample}: {source}")]
    BacktestFailed {
        sample: usize,
        #[source]
        source: RunError,
    },
    #[error("no samples collected")]
    NoSamples,
}

// ─── MC execution ────────────────────────────────────────────────────

/// Run execution Monte Carlo: sample execution parameters and run backtests.
#[allow(clippy::too_many_arguments)]
pub fn run_execution_mc(
    strategy_config: &StrategyConfig,
    aligned: &AlignedData,
    symbol: &str,
    mc_config: &ExecutionMcConfig,
    trading_mode: TradingMode,
    initial_capital: f64,
    position_size_pct: f64,
    dataset_hash: &str,
) -> Result<ExecutionMcResult, McError> {
    let mut rng = StdRng::seed_from_u64(mc_config.seed);
    let mut samples = Vec::with_capacity(mc_config.n_samples);

    for i in 0..mc_config.n_samples {
        let slippage_bps = rng.gen_range(mc_config.slippage_range.0..=mc_config.slippage_range.1);
        let commission_bps =
            rng.gen_range(mc_config.commission_range.0..=mc_config.commission_range.1);
        let policy_idx = rng.gen_range(0..mc_config.path_policies.len());
        let path_policy = mc_config.path_policies[policy_idx];

        let exec_config = ExecutionConfig {
            cost_model: CostModel::new(slippage_bps, commission_bps),
            path_policy,
            gap_policy: GapPolicy::FillAtOpen, // realistic default
            liquidity: None,
        };

        let result = run_backtest_with_exec_config(
            strategy_config,
            aligned,
            symbol,
            trading_mode,
            initial_capital,
            position_size_pct,
            exec_config,
            dataset_hash,
            false,
        )
        .map_err(|e| McError::BacktestFailed {
            sample: i,
            source: e,
        })?;

        samples.push(McSample {
            slippage_bps,
            commission_bps,
            path_policy,
            sharpe: result.metrics.sharpe,
            cagr: result.metrics.cagr,
            max_drawdown: result.metrics.max_drawdown,
            trade_count: result.metrics.trade_count,
        });
    }

    if samples.is_empty() {
        return Err(McError::NoSamples);
    }

    let stability = compute_stability(&samples);

    Ok(ExecutionMcResult { samples, stability })
}

// ─── Stability scoring ───────────────────────────────────────────────

fn compute_stability(samples: &[McSample]) -> StabilityScore {
    let mut sharpes: Vec<f64> = samples.iter().map(|s| s.sharpe).collect();
    sharpes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sharpes.len();
    let median_sharpe = percentile_sorted(&sharpes, 50.0);
    let p25 = percentile_sorted(&sharpes, 25.0);
    let p75 = percentile_sorted(&sharpes, 75.0);
    let p10_sharpe = percentile_sorted(&sharpes, 10.0);
    let iqr_sharpe = p75 - p25;

    // Stability ratio: rewards high median with low variance
    let stability_ratio = if iqr_sharpe.abs() < 1e-15 {
        median_sharpe // No variance — perfect stability
    } else {
        median_sharpe / (1.0 + iqr_sharpe)
    };

    // Check that not all samples are identical
    let all_different = if n > 1 {
        sharpes.windows(2).any(|w| (w[1] - w[0]).abs() > 1e-12)
    } else {
        false
    };

    StabilityScore {
        median_sharpe,
        iqr_sharpe,
        p10_sharpe,
        stability_ratio,
        all_different,
    }
}

/// Percentile of a sorted slice using linear interpolation.
fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return sorted[0];
    }
    let rank = (p / 100.0) * (n - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let frac = rank - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stability_high_variance_penalized() {
        // High-variance set
        let high_var = vec![
            make_sample(3.0),
            make_sample(-1.0),
            make_sample(2.0),
            make_sample(-2.0),
            make_sample(1.0),
        ];
        let s_high = compute_stability(&high_var);

        // Low-variance set with same median
        let low_var = vec![
            make_sample(0.9),
            make_sample(1.0),
            make_sample(1.0),
            make_sample(1.1),
            make_sample(1.0),
        ];
        let s_low = compute_stability(&low_var);

        // Low variance should have better stability ratio
        assert!(
            s_low.stability_ratio > s_high.stability_ratio,
            "Low-var stability {} should > high-var {}",
            s_low.stability_ratio,
            s_high.stability_ratio
        );
    }

    #[test]
    fn stability_all_identical_detected() {
        let identical = vec![make_sample(1.5); 5];
        let s = compute_stability(&identical);
        assert!(!s.all_different);
        assert!((s.iqr_sharpe).abs() < 1e-10);
    }

    #[test]
    fn stability_different_samples_detected() {
        let different = vec![make_sample(1.0), make_sample(2.0), make_sample(3.0)];
        let s = compute_stability(&different);
        assert!(s.all_different);
    }

    #[test]
    fn stability_percentiles_correct() {
        let samples = vec![
            make_sample(1.0),
            make_sample(2.0),
            make_sample(3.0),
            make_sample(4.0),
            make_sample(5.0),
        ];
        let s = compute_stability(&samples);
        assert!((s.median_sharpe - 3.0).abs() < 1e-10);
        assert!(s.p10_sharpe < s.median_sharpe);
    }

    #[test]
    fn default_config_reasonable() {
        let config = ExecutionMcConfig::default();
        assert_eq!(config.n_samples, 20);
        assert_eq!(config.slippage_range, (0.0, 30.0));
        assert_eq!(config.commission_range, (0.0, 20.0));
        assert_eq!(config.path_policies.len(), 3);
    }

    fn make_sample(sharpe: f64) -> McSample {
        McSample {
            slippage_bps: 5.0,
            commission_bps: 5.0,
            path_policy: PathPolicy::WorstCase,
            sharpe,
            cagr: 0.1,
            max_drawdown: -0.1,
            trade_count: 10,
        }
    }
}
