//! Walk-forward validation — train/test fold splitting and OOS evaluation.
//!
//! Splits bar data into expanding in-sample (IS) windows with fixed out-of-sample
//! (OOS) test periods. Each fold trains on IS bars and evaluates on OOS bars.
//! Computes degradation ratio (mean OOS Sharpe / mean IS Sharpe) to detect
//! overfitting.
//!
//! Minimum data requirements:
//! - 756 bars total (3 years)
//! - 252 bars per IS fold
//! - 63 bars per OOS fold (one quarter)

use serde::{Deserialize, Serialize};
use thiserror::Error;

use trendlab_core::components::execution::ExecutionPreset;
use trendlab_core::data::align::AlignedData;
use trendlab_core::fingerprint::{StrategyConfig, TradingMode};

use crate::fdr::TTestResult;
use crate::runner::{run_backtest_from_data, RunError};

// ─── Configuration ───────────────────────────────────────────────────

/// Configuration for walk-forward validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardConfig {
    /// Number of folds (default 5).
    pub n_folds: usize,
    /// Minimum total bars required (default 756 = 3 years).
    pub min_total_bars: usize,
    /// Minimum in-sample bars per fold (default 252 = 1 year).
    pub min_is_bars: usize,
    /// Minimum out-of-sample bars per fold (default 63 = 1 quarter).
    pub min_oos_bars: usize,
}

impl Default for WalkForwardConfig {
    fn default() -> Self {
        Self {
            n_folds: 5,
            min_total_bars: 756,
            min_is_bars: 252,
            min_oos_bars: 63,
        }
    }
}

// ─── Result types ────────────────────────────────────────────────────

/// Specification of a single walk-forward fold (bar index ranges).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldSpec {
    pub fold_index: usize,
    /// In-sample start bar index (inclusive).
    pub is_start: usize,
    /// In-sample end bar index (exclusive).
    pub is_end: usize,
    /// Out-of-sample start bar index (inclusive).
    pub oos_start: usize,
    /// Out-of-sample end bar index (exclusive).
    pub oos_end: usize,
}

/// Result of a single fold evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldResult {
    pub fold_index: usize,
    pub is_sharpe: f64,
    pub oos_sharpe: f64,
    pub is_trades: usize,
    pub oos_trades: usize,
}

/// How the degradation ratio was computed (or why it wasn't).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DegradationFlag {
    /// IS Sharpe >= 0.1, ratio computed normally.
    Normal,
    /// IS Sharpe < 0.1, using difference metric (OOS - IS) instead.
    LowIsSharpe,
    /// IS Sharpe is negative, ratio skipped entirely.
    NegativeIsSharpe,
    /// IS Sharpe positive (>= 0.1) but OOS Sharpe negative: clamped to 0.0.
    FailedOos,
    /// Not enough bars for walk-forward.
    InsufficientData,
}

/// Complete result of walk-forward validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardResult {
    pub fold_results: Vec<FoldResult>,
    pub mean_is_sharpe: f64,
    pub mean_oos_sharpe: f64,
    /// Degradation ratio: mean OOS Sharpe / mean IS Sharpe.
    /// None when ratio cannot be computed (see `degradation_flag`).
    pub degradation_ratio: Option<f64>,
    pub degradation_flag: DegradationFlag,
    /// t-test on fold-level OOS Sharpe values (H0: mean = 0, H1: mean > 0).
    pub t_test: Option<TTestResult>,
}

/// Errors from walk-forward validation.
#[derive(Debug, Error)]
pub enum WalkForwardError {
    #[error("insufficient data: {total_bars} bars < minimum {min_bars}")]
    InsufficientData { total_bars: usize, min_bars: usize },
    #[error("fold creation failed: cannot fit {n_folds} folds in {total_bars} bars")]
    FoldCreationFailed { n_folds: usize, total_bars: usize },
    #[error("backtest error on fold {fold}: {source}")]
    BacktestFailed {
        fold: usize,
        #[source]
        source: RunError,
    },
}

// ─── Fold creation ───────────────────────────────────────────────────

/// Create expanding-window walk-forward fold specifications.
///
/// Each fold has an expanding IS window and a fixed-size OOS window:
/// - Fold 0: IS = [0 .. base_is_end], OOS = [base_is_end .. base_is_end + oos_size]
/// - Fold 1: IS = [0 .. base_is_end + oos_size], OOS = next oos_size bars
/// - etc.
///
/// The IS window grows by one OOS chunk per fold.
pub fn create_folds(
    total_bars: usize,
    config: &WalkForwardConfig,
) -> Result<Vec<FoldSpec>, WalkForwardError> {
    if total_bars < config.min_total_bars {
        return Err(WalkForwardError::InsufficientData {
            total_bars,
            min_bars: config.min_total_bars,
        });
    }

    // OOS size: divide remaining bars (after initial IS) among n_folds
    // Initial IS must be at least min_is_bars
    // Each OOS must be at least min_oos_bars
    let n = config.n_folds;

    // Total bars needed: min_is_bars + n * oos_size <= total_bars
    // oos_size = (total_bars - min_is_bars) / n
    let available_for_oos = total_bars.saturating_sub(config.min_is_bars);
    let oos_size = available_for_oos / n;

    if oos_size < config.min_oos_bars {
        return Err(WalkForwardError::FoldCreationFailed {
            n_folds: n,
            total_bars,
        });
    }

    let base_is_end = config.min_is_bars;

    let mut folds = Vec::with_capacity(n);
    for i in 0..n {
        let is_start = 0;
        let is_end = base_is_end + i * oos_size;
        let oos_start = is_end;
        let oos_end = oos_start + oos_size;

        // Don't create fold if OOS goes past data
        if oos_end > total_bars {
            break;
        }

        folds.push(FoldSpec {
            fold_index: i,
            is_start,
            is_end,
            oos_start,
            oos_end,
        });
    }

    if folds.is_empty() {
        return Err(WalkForwardError::FoldCreationFailed {
            n_folds: n,
            total_bars,
        });
    }

    Ok(folds)
}

/// Slice AlignedData to a bar index range [start, end).
pub fn slice_aligned_data(full: &AlignedData, start: usize, end: usize) -> AlignedData {
    let end = end.min(full.dates.len());
    let start = start.min(end);

    let dates = full.dates[start..end].to_vec();
    let bars = full
        .bars
        .iter()
        .map(|(sym, bars)| (sym.clone(), bars[start..end].to_vec()))
        .collect();

    AlignedData {
        dates,
        bars,
        symbols: full.symbols.clone(),
    }
}

// ─── Walk-forward orchestration ──────────────────────────────────────

/// Run walk-forward validation: split data into folds, backtest each, compute
/// degradation ratio and t-test.
#[allow(clippy::too_many_arguments)]
pub fn run_walk_forward(
    strategy_config: &StrategyConfig,
    aligned: &AlignedData,
    symbol: &str,
    wf_config: &WalkForwardConfig,
    trading_mode: TradingMode,
    initial_capital: f64,
    position_size_pct: f64,
    execution_preset: ExecutionPreset,
    dataset_hash: &str,
) -> Result<WalkForwardResult, WalkForwardError> {
    let total_bars = aligned.dates.len();
    let folds = create_folds(total_bars, wf_config)?;

    let mut fold_results = Vec::with_capacity(folds.len());

    for fold in &folds {
        // Slice data for IS and OOS periods
        let is_data = slice_aligned_data(aligned, fold.is_start, fold.is_end);
        let oos_data = slice_aligned_data(aligned, fold.oos_start, fold.oos_end);

        // Run backtest on IS period
        let is_result = run_backtest_from_data(
            strategy_config,
            &is_data,
            symbol,
            trading_mode,
            initial_capital,
            position_size_pct,
            execution_preset,
            dataset_hash,
            false,
        )
        .map_err(|e| WalkForwardError::BacktestFailed {
            fold: fold.fold_index,
            source: e,
        })?;

        // Run backtest on OOS period
        let oos_result = run_backtest_from_data(
            strategy_config,
            &oos_data,
            symbol,
            trading_mode,
            initial_capital,
            position_size_pct,
            execution_preset,
            dataset_hash,
            false,
        )
        .map_err(|e| WalkForwardError::BacktestFailed {
            fold: fold.fold_index,
            source: e,
        })?;

        fold_results.push(FoldResult {
            fold_index: fold.fold_index,
            is_sharpe: is_result.metrics.sharpe,
            oos_sharpe: oos_result.metrics.sharpe,
            is_trades: is_result.metrics.trade_count,
            oos_trades: oos_result.metrics.trade_count,
        });
    }

    Ok(compute_walk_forward_stats(fold_results))
}

/// Compute aggregate walk-forward statistics from fold results.
fn compute_walk_forward_stats(fold_results: Vec<FoldResult>) -> WalkForwardResult {
    let n = fold_results.len() as f64;
    let mean_is_sharpe = fold_results.iter().map(|f| f.is_sharpe).sum::<f64>() / n;
    let mean_oos_sharpe = fold_results.iter().map(|f| f.oos_sharpe).sum::<f64>() / n;

    // Compute degradation ratio with edge case handling
    let (degradation_ratio, degradation_flag) =
        compute_degradation_ratio(mean_is_sharpe, mean_oos_sharpe);

    // t-test on OOS Sharpe values
    let oos_sharpes: Vec<f64> = fold_results.iter().map(|f| f.oos_sharpe).collect();
    let t_test = crate::fdr::one_sided_t_test(&oos_sharpes);

    WalkForwardResult {
        fold_results,
        mean_is_sharpe,
        mean_oos_sharpe,
        degradation_ratio,
        degradation_flag,
        t_test,
    }
}

/// Compute degradation ratio with proper edge case handling.
///
/// - IS >= 0.1: ratio = OOS / IS (Normal)
/// - IS < 0.1 and >= 0: difference = OOS - IS (LowIsSharpe)
/// - IS < 0: ratio skipped (NegativeIsSharpe)
/// - IS >= 0.1 but OOS < 0: clamped to 0.0 (FailedOos)
fn compute_degradation_ratio(
    mean_is_sharpe: f64,
    mean_oos_sharpe: f64,
) -> (Option<f64>, DegradationFlag) {
    if mean_is_sharpe < 0.0 {
        (None, DegradationFlag::NegativeIsSharpe)
    } else if mean_is_sharpe < 0.1 {
        // Use difference metric instead
        let diff = mean_oos_sharpe - mean_is_sharpe;
        (Some(diff), DegradationFlag::LowIsSharpe)
    } else if mean_oos_sharpe < 0.0 {
        // Positive IS but negative OOS: canonical overfit signature
        (Some(0.0), DegradationFlag::FailedOos)
    } else {
        let ratio = mean_oos_sharpe / mean_is_sharpe;
        (Some(ratio), DegradationFlag::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Fold creation tests ─────────────────────────────────────

    #[test]
    fn create_folds_minimum_data() {
        let config = WalkForwardConfig::default(); // 756 min, 252 IS, 63 OOS, 5 folds
        let folds = create_folds(756, &config).unwrap();

        // With 756 bars: IS starts at 252, OOS size = (756-252)/5 = 100
        assert!(!folds.is_empty());

        // Each fold has IS start at 0
        for fold in &folds {
            assert_eq!(fold.is_start, 0);
        }

        // First fold: IS = [0..252], OOS starts at 252
        assert_eq!(folds[0].is_end, 252);
        assert_eq!(folds[0].oos_start, 252);

        // OOS doesn't go past data
        for fold in &folds {
            assert!(fold.oos_end <= 756);
        }
    }

    #[test]
    fn create_folds_expanding_window() {
        let config = WalkForwardConfig::default();
        let folds = create_folds(1000, &config).unwrap();

        // IS window should expand with each fold
        for i in 1..folds.len() {
            assert!(
                folds[i].is_end > folds[i - 1].is_end,
                "IS window should expand: fold {} is_end={} <= fold {} is_end={}",
                i,
                folds[i].is_end,
                i - 1,
                folds[i - 1].is_end
            );
        }
    }

    #[test]
    fn create_folds_oos_contiguous() {
        let config = WalkForwardConfig::default();
        let folds = create_folds(1000, &config).unwrap();

        // OOS periods should be contiguous (no gaps)
        for i in 1..folds.len() {
            assert_eq!(folds[i].oos_start, folds[i - 1].oos_end);
        }
    }

    #[test]
    fn create_folds_insufficient_data() {
        let config = WalkForwardConfig::default();
        let result = create_folds(500, &config); // < 756 minimum
        assert!(result.is_err());
    }

    #[test]
    fn create_folds_too_many_folds_for_data() {
        let config = WalkForwardConfig {
            n_folds: 20,
            min_total_bars: 756,
            min_is_bars: 700,
            min_oos_bars: 63,
            ..Default::default()
        };
        // 756 bars, 700 IS, only 56 bars left for 20 OOS folds = 2 bars each < 63
        let result = create_folds(756, &config);
        assert!(result.is_err());
    }

    #[test]
    fn create_folds_is_at_least_min_is_bars() {
        let config = WalkForwardConfig::default();
        let folds = create_folds(2000, &config).unwrap();

        for fold in &folds {
            let is_len = fold.is_end - fold.is_start;
            assert!(
                is_len >= config.min_is_bars,
                "IS length {} < minimum {}",
                is_len,
                config.min_is_bars
            );
        }
    }

    #[test]
    fn create_folds_oos_at_least_min_oos_bars() {
        let config = WalkForwardConfig::default();
        let folds = create_folds(2000, &config).unwrap();

        for fold in &folds {
            let oos_len = fold.oos_end - fold.oos_start;
            assert!(
                oos_len >= config.min_oos_bars,
                "OOS length {} < minimum {}",
                oos_len,
                config.min_oos_bars
            );
        }
    }

    // ─── Slice tests ─────────────────────────────────────────────

    #[test]
    fn slice_aligned_data_basic() {
        use chrono::NaiveDate;
        use std::collections::HashMap;
        use trendlab_core::data::provider::RawBar;

        let dates: Vec<NaiveDate> = (0..10)
            .map(|i| NaiveDate::from_ymd_opt(2024, 1, 2 + i).unwrap())
            .collect();

        let bars: Vec<RawBar> = dates
            .iter()
            .enumerate()
            .map(|(i, d)| RawBar {
                date: *d,
                open: 100.0 + i as f64,
                high: 102.0 + i as f64,
                low: 99.0 + i as f64,
                close: 101.0 + i as f64,
                volume: 1000,
                adj_close: 101.0 + i as f64,
            })
            .collect();

        let mut bar_map = HashMap::new();
        bar_map.insert("SPY".to_string(), bars);

        let aligned = AlignedData {
            dates,
            bars: bar_map,
            symbols: vec!["SPY".to_string()],
        };

        let sliced = slice_aligned_data(&aligned, 2, 7);
        assert_eq!(sliced.dates.len(), 5);
        assert_eq!(sliced.bars["SPY"].len(), 5);
        assert_eq!(sliced.bars["SPY"][0].close, 103.0); // index 2 from original
    }

    #[test]
    fn slice_out_of_bounds_clamped() {
        use chrono::NaiveDate;
        use std::collections::HashMap;
        use trendlab_core::data::provider::RawBar;

        let dates = vec![NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()];
        let bars = vec![RawBar {
            date: dates[0],
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 1000,
            adj_close: 101.0,
        }];
        let mut bar_map = HashMap::new();
        bar_map.insert("SPY".to_string(), bars);

        let aligned = AlignedData {
            dates,
            bars: bar_map,
            symbols: vec!["SPY".to_string()],
        };

        // Requesting beyond data — should clamp
        let sliced = slice_aligned_data(&aligned, 0, 100);
        assert_eq!(sliced.dates.len(), 1);
    }

    // ─── Degradation ratio tests ─────────────────────────────────

    #[test]
    fn degradation_normal() {
        let (ratio, flag) = compute_degradation_ratio(2.0, 1.0);
        assert_eq!(flag, DegradationFlag::Normal);
        assert!((ratio.unwrap() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn degradation_perfect() {
        let (ratio, flag) = compute_degradation_ratio(1.5, 1.5);
        assert_eq!(flag, DegradationFlag::Normal);
        assert!((ratio.unwrap() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn degradation_low_is_sharpe() {
        let (ratio, flag) = compute_degradation_ratio(0.05, 0.03);
        assert_eq!(flag, DegradationFlag::LowIsSharpe);
        // Difference metric: 0.03 - 0.05 = -0.02
        assert!((ratio.unwrap() - (-0.02)).abs() < 1e-10);
    }

    #[test]
    fn degradation_negative_is() {
        let (ratio, flag) = compute_degradation_ratio(-0.5, 0.3);
        assert_eq!(flag, DegradationFlag::NegativeIsSharpe);
        assert!(ratio.is_none());
    }

    #[test]
    fn degradation_failed_oos() {
        let (ratio, flag) = compute_degradation_ratio(1.5, -0.3);
        assert_eq!(flag, DegradationFlag::FailedOos);
        assert!((ratio.unwrap() - 0.0).abs() < 1e-10);
    }
}
