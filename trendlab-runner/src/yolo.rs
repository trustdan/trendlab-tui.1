//! YOLO mode — continuous auto-discovery engine.
//!
//! Randomly samples strategy compositions (via the dual-slider sampler),
//! runs backtests across all selected symbols, and maintains a live
//! per-symbol leaderboard of discoveries.
//!
//! Two controls:
//! - `jitter_pct` (0.0–1.0): parameter variation within known structures.
//! - `structural_explore` (0.0–1.0): probability of trying novel component combos.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::NaiveDate;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use trendlab_core::components::sampler::{sample_composition, ComponentPool};
use trendlab_core::domain::RunId;
use trendlab_core::fingerprint::TradingMode;
use trendlab_core::rng::RngHierarchy;

use crate::data_loader::LoadedData;
use crate::fitness::FitnessMetric;
use crate::leaderboard::{LeaderboardEntry, SymbolLeaderboard};
use crate::runner::{decode_execution_preset, run_backtest_from_data, RunError};

// ─── Config types ────────────────────────────────────────────────────

/// Sweep depth: controls parameter grid density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SweepDepth {
    Quick,
    Normal,
    Deep,
}

/// Combo mode: controls multi-strategy composition (stub for now).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComboMode {
    None,
    TwoWay,
    TwoPlusThreeWay,
    All,
}

/// Complete YOLO configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloConfig {
    // ── Dual sliders ──
    pub jitter_pct: f64,
    pub structural_explore: f64,

    // ── Backtest parameters ──
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub initial_capital: f64,
    pub position_size_pct: f64,
    pub trading_mode: TradingMode,

    // ── Robustness (Phase 11 stub) ──
    pub wf_sharpe_threshold: f64,

    // ── Sweep settings ──
    pub sweep_depth: SweepDepth,
    pub warmup_iterations: usize,
    pub combo_mode: ComboMode,

    // ── Threading ──
    pub polars_thread_cap: usize,
    pub outer_thread_cap: usize,

    // ── Limits ──
    pub max_iterations: Option<usize>,
    pub leaderboard_max_size: usize,

    // ── Fitness & seeding ──
    pub fitness_metric: FitnessMetric,
    pub master_seed: u64,
}

impl Default for YoloConfig {
    fn default() -> Self {
        Self {
            jitter_pct: 0.5,
            structural_explore: 0.3,
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            initial_capital: 100_000.0,
            position_size_pct: 1.0,
            trading_mode: TradingMode::LongOnly,
            wf_sharpe_threshold: 0.5,
            sweep_depth: SweepDepth::Normal,
            warmup_iterations: 10,
            combo_mode: ComboMode::None,
            polars_thread_cap: 1,
            outer_thread_cap: 1,
            max_iterations: None,
            leaderboard_max_size: 500,
            fitness_metric: FitnessMetric::Sharpe,
            master_seed: 42,
        }
    }
}

impl YoloConfig {
    /// Enforce the threading mutual exclusion rule:
    /// if outer_thread_cap > 1, force polars_thread_cap = 1.
    ///
    /// Polars internal parallelism is only useful when running a single
    /// backtest in isolation. Under Rayon symbol-level parallelism, nested
    /// Polars threading causes CPU oversubscription.
    pub fn enforce_thread_constraints(&mut self) {
        if self.outer_thread_cap > 1 {
            self.polars_thread_cap = 1;
        }
    }
}

// ─── Progress & result types ─────────────────────────────────────────

/// Progress update sent during YOLO execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloProgress {
    pub iteration: usize,
    pub current_symbol: String,
    pub symbols_complete: usize,
    pub symbols_total: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub throughput_per_min: f64,
    pub leaderboard_entries: usize,
    pub elapsed_secs: f64,
}

/// Final result of a YOLO run.
pub struct YoloResult {
    pub leaderboards: HashMap<String, SymbolLeaderboard>,
    pub iterations_completed: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub elapsed_secs: f64,
}

/// Errors from the YOLO engine.
#[derive(Debug, Error)]
pub enum YoloError {
    #[error("no symbols provided")]
    NoSymbols,
    #[error("data error: {0}")]
    Data(String),
}

/// Record of a failed iteration for diagnostics.
#[derive(Debug)]
#[allow(dead_code)]
struct FailedIteration {
    iteration: usize,
    symbol: String,
    error: String,
}

// ─── Core YOLO loop ──────────────────────────────────────────────────

/// Run YOLO mode: continuously sample strategies and populate per-symbol leaderboards.
///
/// # Arguments
/// - `config`: YOLO configuration (sliders, threading, limits, seed).
/// - `data`: Pre-loaded bar data for all symbols.
/// - `symbols`: List of symbols to test each iteration.
/// - `progress_cb`: Optional callback for progress updates (throttled to ~500ms).
/// - `cancel`: Optional atomic flag to stop the loop cooperatively.
pub fn run_yolo(
    config: &YoloConfig,
    data: &LoadedData,
    symbols: &[String],
    progress_cb: Option<&dyn Fn(&YoloProgress)>,
    cancel: Option<&AtomicBool>,
) -> Result<YoloResult, YoloError> {
    if symbols.is_empty() {
        return Err(YoloError::NoSymbols);
    }

    let mut config = config.clone();
    config.enforce_thread_constraints();

    let start_time = Instant::now();
    let pool = ComponentPool::default_pool();
    let run_id = RunId::from_bytes(format!("yolo-{}", config.master_seed).as_bytes());
    let rng_hierarchy = RngHierarchy::new(config.master_seed);
    let session_id = format!(
        "yolo-{}-{}",
        config.master_seed,
        chrono::Utc::now().timestamp()
    );

    // Initialize per-symbol leaderboards
    let mut leaderboards: HashMap<String, SymbolLeaderboard> = symbols
        .iter()
        .map(|s| {
            (
                s.clone(),
                SymbolLeaderboard::new(
                    s.clone(),
                    config.leaderboard_max_size,
                    config.fitness_metric,
                ),
            )
        })
        .collect();

    let mut success_count: usize = 0;
    let mut error_count: usize = 0;
    let mut failed_log: Vec<FailedIteration> = Vec::new();
    let mut last_progress = Instant::now();

    // Build Rayon thread pool if outer_thread_cap > 1
    let thread_pool = if config.outer_thread_cap > 1 {
        Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(config.outer_thread_cap)
                .build()
                .expect("failed to build Rayon thread pool"),
        )
    } else {
        None
    };

    let mut iteration: usize = 0;

    loop {
        // Check cancellation
        if cancel.is_some_and(|f| f.load(Ordering::Relaxed)) {
            break;
        }

        // Check iteration limit
        if let Some(max) = config.max_iterations {
            if iteration >= max {
                break;
            }
        }

        // Sample a strategy config using the iteration-specific RNG
        let mut sampler_rng = rng_hierarchy.rng_for(&run_id, "sampler", iteration as u64);
        let strategy_config = sample_composition(
            &pool,
            &mut sampler_rng,
            config.jitter_pct,
            config.structural_explore,
        );

        // Decode execution preset from the sampled config
        let iter_preset = decode_execution_preset(&strategy_config.execution_model.params);

        // Run backtests for each symbol
        let iter_results: Vec<(String, Result<crate::runner::BacktestResult, RunError>)> =
            if let Some(ref tp) = thread_pool {
                tp.install(|| {
                    symbols
                        .par_iter()
                        .map(|symbol| {
                            let result = run_backtest_from_data(
                                &strategy_config,
                                &data.aligned,
                                symbol,
                                config.trading_mode,
                                config.initial_capital,
                                config.position_size_pct,
                                iter_preset,
                                &data.dataset_hash,
                                data.has_synthetic,
                            );
                            (symbol.clone(), result)
                        })
                        .collect()
                })
            } else {
                symbols
                    .iter()
                    .map(|symbol| {
                        let result = run_backtest_from_data(
                            &strategy_config,
                            &data.aligned,
                            symbol,
                            config.trading_mode,
                            config.initial_capital,
                            config.position_size_pct,
                            iter_preset,
                            &data.dataset_hash,
                            data.has_synthetic,
                        );
                        (symbol.clone(), result)
                    })
                    .collect()
            };

        // Process results
        let now = chrono::Utc::now().naive_utc();
        for (symbol, result) in iter_results {
            match result {
                Ok(backtest_result) => {
                    let fitness = config.fitness_metric.extract(&backtest_result.metrics);

                    // Filter: at least 1 trade and finite metrics
                    if backtest_result.trades.is_empty()
                        || !fitness.is_finite()
                        || !backtest_result.metrics.sharpe.is_finite()
                        || !backtest_result.metrics.cagr.is_finite()
                    {
                        success_count += 1; // Counts as successful execution, just not leaderboard-worthy
                        continue;
                    }

                    let entry = LeaderboardEntry {
                        result: backtest_result,
                        fitness_score: fitness,
                        iteration,
                        session_id: session_id.clone(),
                        timestamp: now,
                    };

                    if let Some(lb) = leaderboards.get_mut(&symbol) {
                        lb.insert(entry);
                    }
                    success_count += 1;
                }
                Err(e) => {
                    failed_log.push(FailedIteration {
                        iteration,
                        symbol: symbol.clone(),
                        error: e.to_string(),
                    });
                    error_count += 1;
                }
            }
        }

        // Progress callback (throttled to 500ms)
        if let Some(cb) = progress_cb {
            if last_progress.elapsed().as_millis() >= 500 || iteration == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let total_lb_entries: usize = leaderboards.values().map(|lb| lb.len()).sum();
                let throughput = if elapsed > 0.0 {
                    success_count as f64 / (elapsed / 60.0)
                } else {
                    0.0
                };

                cb(&YoloProgress {
                    iteration,
                    current_symbol: String::new(),
                    symbols_complete: symbols.len(),
                    symbols_total: symbols.len(),
                    success_count,
                    error_count,
                    throughput_per_min: throughput,
                    leaderboard_entries: total_lb_entries,
                    elapsed_secs: elapsed,
                });
                last_progress = Instant::now();
            }
        }

        iteration += 1;
    }

    let elapsed = start_time.elapsed().as_secs_f64();

    Ok(YoloResult {
        leaderboards,
        iterations_completed: iteration,
        success_count,
        error_count,
        elapsed_secs: elapsed,
    })
}

/// Check if a backtest result has valid metrics for leaderboard consideration.
pub fn is_valid_for_leaderboard(
    metrics: &crate::metrics::PerformanceMetrics,
    trade_count: usize,
) -> bool {
    trade_count >= 1
        && metrics.sharpe.is_finite()
        && metrics.cagr.is_finite()
        && metrics.max_drawdown.is_finite()
        && metrics.profit_factor.is_finite()
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_reasonable() {
        let config = YoloConfig::default();
        assert_eq!(config.jitter_pct, 0.5);
        assert_eq!(config.structural_explore, 0.3);
        assert_eq!(config.initial_capital, 100_000.0);
        assert_eq!(config.master_seed, 42);
        assert_eq!(config.leaderboard_max_size, 500);
        assert_eq!(config.fitness_metric, FitnessMetric::Sharpe);
    }

    #[test]
    fn thread_constraint_enforced() {
        let mut config = YoloConfig::default();
        config.outer_thread_cap = 4;
        config.polars_thread_cap = 4;
        config.enforce_thread_constraints();
        assert_eq!(config.polars_thread_cap, 1);
    }

    #[test]
    fn thread_constraint_not_applied_for_single_thread() {
        let mut config = YoloConfig::default();
        config.outer_thread_cap = 1;
        config.polars_thread_cap = 4;
        config.enforce_thread_constraints();
        assert_eq!(config.polars_thread_cap, 4);
    }

    #[test]
    fn no_symbols_returns_error() {
        let config = YoloConfig {
            max_iterations: Some(1),
            ..YoloConfig::default()
        };
        let data = LoadedData {
            aligned: trendlab_core::data::align::AlignedData {
                dates: vec![],
                bars: HashMap::new(),
                symbols: vec![],
            },
            sources: HashMap::new(),
            dataset_hash: "empty".into(),
            has_synthetic: false,
        };
        let result = run_yolo(&config, &data, &[], None, None);
        assert!(result.is_err());
    }

    #[test]
    fn sweep_depth_serialization() {
        let depth = SweepDepth::Deep;
        let json = serde_json::to_string(&depth).unwrap();
        let deser: SweepDepth = serde_json::from_str(&json).unwrap();
        assert_eq!(depth, deser);
    }

    #[test]
    fn combo_mode_serialization() {
        let mode = ComboMode::None;
        let json = serde_json::to_string(&mode).unwrap();
        let deser: ComboMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deser);
    }

    #[test]
    fn is_valid_for_leaderboard_rejects_zero_trades() {
        let metrics = crate::metrics::PerformanceMetrics {
            total_return: 0.0,
            cagr: 0.0,
            sharpe: 0.0,
            sortino: 0.0,
            calmar: 0.0,
            max_drawdown: 0.0,
            win_rate: 0.0,
            profit_factor: 0.0,
            trade_count: 0,
            turnover: 0.0,
            max_consecutive_wins: 0,
            max_consecutive_losses: 0,
            avg_losing_streak: 0.0,
        };
        assert!(!is_valid_for_leaderboard(&metrics, 0));
    }

    #[test]
    fn is_valid_for_leaderboard_rejects_nan() {
        let metrics = crate::metrics::PerformanceMetrics {
            total_return: 0.1,
            cagr: f64::NAN,
            sharpe: 1.0,
            sortino: 1.0,
            calmar: 0.5,
            max_drawdown: -0.1,
            win_rate: 0.5,
            profit_factor: 1.5,
            trade_count: 5,
            turnover: 2.0,
            max_consecutive_wins: 3,
            max_consecutive_losses: 2,
            avg_losing_streak: 1.5,
        };
        assert!(!is_valid_for_leaderboard(&metrics, 5));
    }

    #[test]
    fn is_valid_for_leaderboard_accepts_good_metrics() {
        let metrics = crate::metrics::PerformanceMetrics {
            total_return: 0.1,
            cagr: 0.08,
            sharpe: 1.5,
            sortino: 2.0,
            calmar: 0.5,
            max_drawdown: -0.1,
            win_rate: 0.5,
            profit_factor: 1.5,
            trade_count: 10,
            turnover: 2.0,
            max_consecutive_wins: 3,
            max_consecutive_losses: 2,
            avg_losing_streak: 1.5,
        };
        assert!(is_valid_for_leaderboard(&metrics, 10));
    }
}
