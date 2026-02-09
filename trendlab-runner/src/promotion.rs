//! Promotion ladder — sequential robustness levels for strategy candidates.
//!
//! Cheap candidates must "earn" expensive simulation:
//! - **Level 1 (Cheap Pass):** single backtest passed basic filters.
//! - **Level 2 (Walk-Forward):** OOS performance survives walk-forward validation.
//! - **Level 3 (Execution MC + Bootstrap):** execution sensitivity is bounded; Sharpe CI is graded.
//!
//! The `promote()` function orchestrates the gates: each level runs only if the
//! previous level passed. OOS p-values are recorded into an `FdrFamily` for
//! Benjamini-Hochberg correction across the YOLO run.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use trendlab_core::components::execution::ExecutionPreset;
use trendlab_core::data::align::AlignedData;
use trendlab_core::fingerprint::{StrategyConfig, TradingMode};

use crate::bootstrap::{
    stationary_block_bootstrap, BootstrapConfig, BootstrapError, BootstrapResult,
};
use crate::execution_mc::{
    run_execution_mc, ExecutionMcConfig, ExecutionMcResult, McError,
};
use crate::fdr::FdrFamily;
use crate::runner::BacktestResult;
use crate::walk_forward::{
    run_walk_forward, DegradationFlag, WalkForwardConfig, WalkForwardError, WalkForwardResult,
};

// ─── Configuration ───────────────────────────────────────────────────

/// Configuration for the promotion ladder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionConfig {
    /// Minimum Sharpe from Level 1 backtest to attempt Level 2 walk-forward.
    pub wf_sharpe_threshold: f64,
    /// Walk-forward configuration.
    pub wf_config: WalkForwardConfig,
    /// Minimum degradation ratio to pass walk-forward gate (Level 2 → 3).
    pub wf_degradation_threshold: f64,
    /// Execution Monte Carlo configuration.
    pub mc_config: ExecutionMcConfig,
    /// Bootstrap configuration.
    pub bootstrap_config: BootstrapConfig,
    /// FDR significance level (default 0.05).
    pub fdr_alpha: f64,
}

impl Default for PromotionConfig {
    fn default() -> Self {
        Self {
            wf_sharpe_threshold: 0.3,
            wf_config: WalkForwardConfig::default(),
            wf_degradation_threshold: 0.3,
            mc_config: ExecutionMcConfig::default(),
            bootstrap_config: BootstrapConfig::default(),
            fdr_alpha: 0.05,
        }
    }
}

// ─── Result types ────────────────────────────────────────────────────

/// How far a strategy progressed through the promotion ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PromotionLevel {
    /// Single backtest passed (cheap pass).
    Level1CheapPass,
    /// Walk-forward validation passed.
    Level2WalkForward,
    /// Execution MC + bootstrap passed.
    Level3ExecutionMc,
}

/// Complete robustness result from the promotion pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobustnessResult {
    /// Highest level reached.
    pub level_reached: PromotionLevel,
    /// Walk-forward result (None if Level 1 gate failed).
    pub walk_forward: Option<WalkForwardResult>,
    /// Execution MC result (None if Level 2 gate failed).
    pub execution_mc: Option<ExecutionMcResult>,
    /// Bootstrap result (None if Level 2 gate failed).
    pub bootstrap: Option<BootstrapResult>,
    /// Reason promotion stopped (None if reached Level 3).
    pub gate_failure: Option<GateFailure>,
}

/// Why promotion stopped at a particular level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateFailure {
    /// Level 1 Sharpe below threshold.
    InsufficientSharpe { sharpe: f64, threshold: f64 },
    /// Walk-forward degradation too high or OOS failed.
    WalkForwardFailed { reason: String },
    /// Walk-forward error (insufficient data, backtest failure, etc.).
    WalkForwardError { reason: String },
}

/// Errors from the promotion pipeline.
#[derive(Debug, Error)]
pub enum PromotionError {
    #[error("walk-forward error: {0}")]
    WalkForward(#[from] WalkForwardError),
    #[error("execution MC error: {0}")]
    ExecutionMc(#[from] McError),
    #[error("bootstrap error: {0}")]
    Bootstrap(#[from] BootstrapError),
}

// ─── Promotion orchestration ─────────────────────────────────────────

/// Run the promotion ladder for a strategy that passed Level 1.
///
/// Gate logic:
/// - **1 → 2:** Level 1 Sharpe >= `wf_sharpe_threshold`.
/// - **2 → 3:** Degradation ratio > `wf_degradation_threshold` (when Normal),
///   OOS Sharpe > 0, and p-value is recorded into `fdr_family`.
/// - **Level 3:** Run execution MC + bootstrap. Always completes if Level 2 passes.
///
/// The `fdr_family` accumulates OOS p-values across all promoted strategies
/// in the YOLO run for Benjamini-Hochberg correction.
#[allow(clippy::too_many_arguments)]
pub fn promote(
    result: &BacktestResult,
    strategy_config: &StrategyConfig,
    aligned: &AlignedData,
    symbol: &str,
    trading_mode: TradingMode,
    initial_capital: f64,
    position_size_pct: f64,
    execution_preset: ExecutionPreset,
    dataset_hash: &str,
    promotion_config: &PromotionConfig,
    fdr_family: &mut FdrFamily,
) -> RobustnessResult {
    // ── Gate 1 → 2: Sharpe threshold ──
    let sharpe = result.metrics.sharpe;
    if sharpe < promotion_config.wf_sharpe_threshold {
        return RobustnessResult {
            level_reached: PromotionLevel::Level1CheapPass,
            walk_forward: None,
            execution_mc: None,
            bootstrap: None,
            gate_failure: Some(GateFailure::InsufficientSharpe {
                sharpe,
                threshold: promotion_config.wf_sharpe_threshold,
            }),
        };
    }

    // ── Level 2: Walk-Forward ──
    let wf_result = match run_walk_forward(
        strategy_config,
        aligned,
        symbol,
        &promotion_config.wf_config,
        trading_mode,
        initial_capital,
        position_size_pct,
        execution_preset,
        dataset_hash,
    ) {
        Ok(wf) => wf,
        Err(e) => {
            return RobustnessResult {
                level_reached: PromotionLevel::Level1CheapPass,
                walk_forward: None,
                execution_mc: None,
                bootstrap: None,
                gate_failure: Some(GateFailure::WalkForwardError {
                    reason: e.to_string(),
                }),
            };
        }
    };

    // Record p-value into FDR family if t-test produced one
    if let Some(ref t_test) = wf_result.t_test {
        let config_id = format!("{:?}", strategy_config);
        fdr_family.add(config_id, t_test.p_value);
    }

    // Check walk-forward gate
    if !passes_wf_gate(&wf_result, promotion_config) {
        let reason = wf_gate_failure_reason(&wf_result, promotion_config);
        return RobustnessResult {
            level_reached: PromotionLevel::Level2WalkForward,
            walk_forward: Some(wf_result),
            execution_mc: None,
            bootstrap: None,
            gate_failure: Some(GateFailure::WalkForwardFailed { reason }),
        };
    }

    // ── Level 3: Execution MC + Bootstrap ──
    let mc_result = run_execution_mc(
        strategy_config,
        aligned,
        symbol,
        &promotion_config.mc_config,
        trading_mode,
        initial_capital,
        position_size_pct,
        dataset_hash,
    )
    .ok();

    let bootstrap_result =
        stationary_block_bootstrap(&result.equity_curve, &promotion_config.bootstrap_config).ok();

    RobustnessResult {
        level_reached: PromotionLevel::Level3ExecutionMc,
        walk_forward: Some(wf_result),
        execution_mc: mc_result,
        bootstrap: bootstrap_result,
        gate_failure: None,
    }
}

// ─── Gate helpers ────────────────────────────────────────────────────

/// Check if walk-forward result passes the Level 2 → 3 gate.
fn passes_wf_gate(wf: &WalkForwardResult, config: &PromotionConfig) -> bool {
    match wf.degradation_flag {
        DegradationFlag::Normal => {
            // Degradation ratio must exceed threshold and OOS must be positive
            if let Some(ratio) = wf.degradation_ratio {
                ratio > config.wf_degradation_threshold && wf.mean_oos_sharpe > 0.0
            } else {
                false
            }
        }
        DegradationFlag::LowIsSharpe => {
            // Difference metric: OOS must be positive
            wf.mean_oos_sharpe > 0.0
        }
        DegradationFlag::NegativeIsSharpe | DegradationFlag::FailedOos => false,
        DegradationFlag::InsufficientData => false,
    }
}

/// Human-readable reason why walk-forward gate failed.
fn wf_gate_failure_reason(wf: &WalkForwardResult, config: &PromotionConfig) -> String {
    match wf.degradation_flag {
        DegradationFlag::Normal => {
            if let Some(ratio) = wf.degradation_ratio {
                if ratio <= config.wf_degradation_threshold {
                    format!(
                        "degradation ratio {:.3} <= threshold {:.3}",
                        ratio, config.wf_degradation_threshold
                    )
                } else {
                    format!("mean OOS Sharpe {:.3} <= 0", wf.mean_oos_sharpe)
                }
            } else {
                "degradation ratio unavailable".into()
            }
        }
        DegradationFlag::LowIsSharpe => {
            format!(
                "low IS Sharpe + mean OOS Sharpe {:.3} <= 0",
                wf.mean_oos_sharpe
            )
        }
        DegradationFlag::NegativeIsSharpe => "negative IS Sharpe".into(),
        DegradationFlag::FailedOos => "positive IS but negative OOS".into(),
        DegradationFlag::InsufficientData => "insufficient data for walk-forward".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PromotionLevel ordering ──────────────────────────────────

    #[test]
    fn level_ordering() {
        assert!(PromotionLevel::Level1CheapPass < PromotionLevel::Level2WalkForward);
        assert!(PromotionLevel::Level2WalkForward < PromotionLevel::Level3ExecutionMc);
    }

    // ─── Default config ───────────────────────────────────────────

    #[test]
    fn default_config_reasonable() {
        let config = PromotionConfig::default();
        assert!((config.wf_sharpe_threshold - 0.3).abs() < 1e-10);
        assert!((config.wf_degradation_threshold - 0.3).abs() < 1e-10);
        assert!((config.fdr_alpha - 0.05).abs() < 1e-10);
        assert_eq!(config.wf_config.n_folds, 5);
        assert_eq!(config.mc_config.n_samples, 20);
        assert_eq!(config.bootstrap_config.n_resamples, 1000);
    }

    // ─── WF gate logic ───────────────────────────────────────────

    #[test]
    fn wf_gate_normal_passes() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::Normal, Some(0.8), 0.5);
        assert!(passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_normal_low_ratio() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::Normal, Some(0.2), 0.5);
        assert!(!passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_normal_negative_oos() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::Normal, Some(0.8), -0.1);
        assert!(!passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_low_is_sharpe_positive_oos() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::LowIsSharpe, None, 0.3);
        assert!(passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_low_is_sharpe_negative_oos() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::LowIsSharpe, None, -0.1);
        assert!(!passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_failed_oos_blocked() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::FailedOos, Some(0.0), -0.1);
        assert!(!passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_negative_is_blocked() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::NegativeIsSharpe, None, 0.5);
        assert!(!passes_wf_gate(&wf, &config));
    }

    #[test]
    fn wf_gate_insufficient_data_blocked() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::InsufficientData, None, 0.0);
        assert!(!passes_wf_gate(&wf, &config));
    }

    // ─── Gate failure reasons ─────────────────────────────────────

    #[test]
    fn failure_reason_low_ratio() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::Normal, Some(0.1), 0.5);
        let reason = wf_gate_failure_reason(&wf, &config);
        assert!(reason.contains("degradation ratio"));
    }

    #[test]
    fn failure_reason_negative_is() {
        let config = PromotionConfig::default();
        let wf = make_wf_result(DegradationFlag::NegativeIsSharpe, None, 0.5);
        let reason = wf_gate_failure_reason(&wf, &config);
        assert!(reason.contains("negative IS"));
    }

    fn make_wf_result(
        flag: DegradationFlag,
        ratio: Option<f64>,
        mean_oos: f64,
    ) -> WalkForwardResult {
        WalkForwardResult {
            fold_results: vec![],
            mean_is_sharpe: 1.0,
            mean_oos_sharpe: mean_oos,
            degradation_ratio: ratio,
            degradation_flag: flag,
            t_test: None,
        }
    }
}
