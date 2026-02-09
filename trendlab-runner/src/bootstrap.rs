//! Block bootstrap confidence grading — stationary block bootstrap for Sharpe CI.
//!
//! Uses a stationary block bootstrap (geometric block length distribution) to
//! preserve autocorrelation structure in financial returns. Builds a confidence
//! interval for the annualized Sharpe ratio and assigns a grade.
//!
//! Key design choices:
//! - Mean block length of 20 trading days (≈1 month) preserves serial dependence.
//! - Geometric distribution for block lengths (Politis & Romano, 1994).
//! - Minimum 250 daily return observations required.
//! - Cross-symbol bootstrap constructs a portfolio equity curve from per-symbol curves.

use std::collections::HashMap;

use chrono::NaiveDate;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::metrics::daily_returns;

// ─── Configuration ───────────────────────────────────────────────────

/// Configuration for block bootstrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Number of bootstrap resamples (default 1000).
    pub n_resamples: usize,
    /// Mean block length in trading days (default 20).
    pub mean_block_length: usize,
    /// RNG seed for reproducibility.
    pub seed: u64,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            n_resamples: 1000,
            mean_block_length: 20,
            seed: 42,
        }
    }
}

// ─── Result types ────────────────────────────────────────────────────

/// Confidence grade for a leaderboard entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceGrade {
    /// CI lower bound strongly positive (> 0.5), CI reasonably narrow (< 3.0).
    High,
    /// CI lower bound positive (> 0.0), CI moderate (< 5.0).
    Medium,
    /// CI wide or lower bound near zero.
    Low,
    /// Too few observations to grade (< 250 daily returns).
    Insufficient,
}

/// Result of a block bootstrap analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResult {
    pub grade: ConfidenceGrade,
    /// 5th percentile of bootstrap Sharpe distribution (CI lower bound).
    pub sharpe_ci_lower: f64,
    /// 95th percentile of bootstrap Sharpe distribution (CI upper bound).
    pub sharpe_ci_upper: f64,
    /// Median of bootstrap Sharpe distribution.
    pub sharpe_median: f64,
    /// Width of the 90% CI.
    pub ci_width: f64,
    pub n_resamples: usize,
    pub sample_size: usize,
}

/// Cross-symbol bootstrap result: portfolio-level + per-symbol diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSymbolBootstrapResult {
    /// Primary grade (portfolio-level bootstrap).
    pub portfolio_level: BootstrapResult,
    /// Secondary diagnostic (per-symbol analysis).
    pub per_symbol_diagnostic: PerSymbolDiagnostic,
}

/// Per-symbol diagnostic: identifies whether performance is concentrated or broad.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerSymbolDiagnostic {
    pub mean_sharpe: f64,
    pub worst_sharpe: f64,
    pub symbol_count: usize,
    pub hit_rate: f64,
    /// Meets minimum guardrails (>= 3 symbols, worst > -1.0, hit_rate >= 0.3).
    pub adequate: bool,
}

/// Errors from bootstrap.
#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("insufficient data: {sample_size} daily returns < minimum 250")]
    InsufficientData { sample_size: usize },
    #[error("insufficient overlap: {overlap_bars} common bars < minimum 250")]
    InsufficientOverlap { overlap_bars: usize },
    #[error("no symbols provided")]
    NoSymbols,
}

// ─── Single-series bootstrap ─────────────────────────────────────────

/// Run stationary block bootstrap on daily returns from an equity curve.
///
/// Requires >= 250 daily return observations. Returns a bootstrapped CI for
/// the annualized Sharpe ratio and a confidence grade.
pub fn stationary_block_bootstrap(
    equity_curve: &[f64],
    config: &BootstrapConfig,
) -> Result<BootstrapResult, BootstrapError> {
    let returns = daily_returns(equity_curve);
    let n = returns.len();

    if n < 250 {
        return Err(BootstrapError::InsufficientData { sample_size: n });
    }

    bootstrap_from_returns(&returns, config)
}

/// Run bootstrap directly on a return series.
fn bootstrap_from_returns(
    returns: &[f64],
    config: &BootstrapConfig,
) -> Result<BootstrapResult, BootstrapError> {
    let n = returns.len();
    if n < 250 {
        return Err(BootstrapError::InsufficientData { sample_size: n });
    }

    let mut rng = StdRng::seed_from_u64(config.seed);
    let p = 1.0 / config.mean_block_length.max(1) as f64; // geometric parameter

    let mut bootstrap_sharpes = Vec::with_capacity(config.n_resamples);

    for _ in 0..config.n_resamples {
        let resampled = resample_stationary_block(returns, n, p, &mut rng);
        let sharpe = annualized_sharpe(&resampled);
        if sharpe.is_finite() {
            bootstrap_sharpes.push(sharpe);
        }
    }

    if bootstrap_sharpes.is_empty() {
        return Ok(BootstrapResult {
            grade: ConfidenceGrade::Insufficient,
            sharpe_ci_lower: 0.0,
            sharpe_ci_upper: 0.0,
            sharpe_median: 0.0,
            ci_width: 0.0,
            n_resamples: 0,
            sample_size: n,
        });
    }

    bootstrap_sharpes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let ci_lower = percentile_sorted(&bootstrap_sharpes, 5.0);
    let ci_upper = percentile_sorted(&bootstrap_sharpes, 95.0);
    let median = percentile_sorted(&bootstrap_sharpes, 50.0);
    let ci_width = ci_upper - ci_lower;

    let grade = assign_grade(ci_lower, ci_width);

    Ok(BootstrapResult {
        grade,
        sharpe_ci_lower: ci_lower,
        sharpe_ci_upper: ci_upper,
        sharpe_median: median,
        ci_width,
        n_resamples: bootstrap_sharpes.len(),
        sample_size: n,
    })
}

/// Generate one stationary block bootstrap resample.
///
/// Uses geometric block lengths with parameter p = 1/mean_block_length.
/// At each step: with probability p, start a new random block; otherwise
/// continue the current block (wrapping around).
fn resample_stationary_block(
    returns: &[f64],
    target_len: usize,
    p: f64,
    rng: &mut StdRng,
) -> Vec<f64> {
    let n = returns.len();
    let mut resampled = Vec::with_capacity(target_len);
    let mut pos = rng.gen_range(0..n);

    for _ in 0..target_len {
        resampled.push(returns[pos]);
        // With probability p, jump to a new random position
        if rng.gen::<f64>() < p {
            pos = rng.gen_range(0..n);
        } else {
            pos = (pos + 1) % n; // continue current block, wrap around
        }
    }

    resampled
}

/// Compute annualized Sharpe ratio from daily returns.
fn annualized_sharpe(returns: &[f64]) -> f64 {
    let n = returns.len();
    if n < 2 {
        return 0.0;
    }
    let mean = returns.iter().sum::<f64>() / n as f64;
    let variance = returns.iter().map(|&r| (r - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    let std = variance.sqrt();

    if std < 1e-15 {
        return 0.0;
    }

    (mean / std) * 252.0_f64.sqrt()
}

/// Assign confidence grade based on CI lower bound and width.
///
/// Thresholds account for naturally wider CIs from stationary block bootstrap
/// (geometric block lengths inflate CI vs IID assumption).
fn assign_grade(ci_lower: f64, ci_width: f64) -> ConfidenceGrade {
    if ci_lower > 0.5 && ci_width < 3.0 {
        ConfidenceGrade::High
    } else if ci_lower > 0.0 && ci_width < 5.0 {
        ConfidenceGrade::Medium
    } else {
        ConfidenceGrade::Low
    }
}

// ─── Cross-symbol bootstrap ─────────────────────────────────────────

/// Run cross-symbol bootstrap: portfolio-level + per-symbol diagnostic.
///
/// Constructs a synthetic equally-weighted portfolio from per-symbol equity curves,
/// truncated to the common date range. Requires >= 250 bars of overlap.
pub fn cross_symbol_bootstrap(
    symbol_equity_curves: &HashMap<String, Vec<f64>>,
    symbol_dates: &HashMap<String, Vec<NaiveDate>>,
    config: &BootstrapConfig,
) -> Result<CrossSymbolBootstrapResult, BootstrapError> {
    if symbol_equity_curves.is_empty() {
        return Err(BootstrapError::NoSymbols);
    }

    // Find common date range (intersection)
    let common_dates = find_common_dates(symbol_dates);
    if common_dates.len() < 250 {
        return Err(BootstrapError::InsufficientOverlap {
            overlap_bars: common_dates.len(),
        });
    }

    // Build portfolio equity curve (equally weighted)
    let portfolio_equity =
        build_portfolio_equity(symbol_equity_curves, symbol_dates, &common_dates);

    // Portfolio-level bootstrap
    let portfolio_level = stationary_block_bootstrap(&portfolio_equity, config)?;

    // Per-symbol diagnostic
    let per_symbol_diagnostic = compute_per_symbol_diagnostic(symbol_equity_curves);

    Ok(CrossSymbolBootstrapResult {
        portfolio_level,
        per_symbol_diagnostic,
    })
}

/// Find dates present in ALL symbols.
fn find_common_dates(symbol_dates: &HashMap<String, Vec<NaiveDate>>) -> Vec<NaiveDate> {
    let mut iter = symbol_dates.values();
    let first = match iter.next() {
        Some(dates) => dates
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        None => return Vec::new(),
    };

    let common = iter.fold(first, |acc, dates| {
        let set: std::collections::BTreeSet<_> = dates.iter().copied().collect();
        acc.intersection(&set).copied().collect()
    });

    common.into_iter().collect()
}

/// Build an equally-weighted portfolio equity curve on the common dates.
fn build_portfolio_equity(
    equity_curves: &HashMap<String, Vec<f64>>,
    symbol_dates: &HashMap<String, Vec<NaiveDate>>,
    common_dates: &[NaiveDate],
) -> Vec<f64> {
    let n_symbols = equity_curves.len() as f64;
    let mut portfolio = vec![0.0; common_dates.len()];

    for (symbol, curve) in equity_curves {
        let dates = match symbol_dates.get(symbol) {
            Some(d) => d,
            None => continue,
        };

        // Build date → equity lookup
        let date_to_equity: HashMap<NaiveDate, f64> = dates
            .iter()
            .zip(curve.iter())
            .map(|(&d, &e)| (d, e))
            .collect();

        // Normalize: each symbol starts at 1.0 (equal weight)
        let first_equity = common_dates
            .first()
            .and_then(|d| date_to_equity.get(d))
            .copied()
            .unwrap_or(1.0);

        for (i, date) in common_dates.iter().enumerate() {
            let equity = date_to_equity.get(date).copied().unwrap_or(first_equity);
            let normalized = equity / first_equity;
            portfolio[i] += normalized / n_symbols;
        }
    }

    portfolio
}

/// Compute per-symbol Sharpe diagnostic.
fn compute_per_symbol_diagnostic(
    equity_curves: &HashMap<String, Vec<f64>>,
) -> PerSymbolDiagnostic {
    let mut sharpes = Vec::new();
    let mut profitable_count = 0;

    for curve in equity_curves.values() {
        let returns = daily_returns(curve);
        let sharpe = annualized_sharpe(&returns);
        if sharpe.is_finite() {
            sharpes.push(sharpe);
        }
        if let (Some(&first), Some(&last)) = (curve.first(), curve.last()) {
            if last > first {
                profitable_count += 1;
            }
        }
    }

    let symbol_count = sharpes.len();
    let mean_sharpe = if symbol_count > 0 {
        sharpes.iter().sum::<f64>() / symbol_count as f64
    } else {
        0.0
    };
    let worst_sharpe = sharpes
        .iter()
        .copied()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);
    let hit_rate = if symbol_count > 0 {
        profitable_count as f64 / symbol_count as f64
    } else {
        0.0
    };

    let adequate = symbol_count >= 3 && worst_sharpe > -1.0 && hit_rate >= 0.3;

    PerSymbolDiagnostic {
        mean_sharpe,
        worst_sharpe,
        symbol_count,
        hit_rate,
        adequate,
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

    // ─── Grade assignment ────────────────────────────────────────

    #[test]
    fn grade_high() {
        assert_eq!(assign_grade(0.8, 0.5), ConfidenceGrade::High);
    }

    #[test]
    fn grade_medium() {
        assert_eq!(assign_grade(0.3, 1.5), ConfidenceGrade::Medium);
    }

    #[test]
    fn grade_low_wide_ci() {
        assert_eq!(assign_grade(0.8, 6.0), ConfidenceGrade::Low);
    }

    #[test]
    fn grade_low_negative_lower() {
        assert_eq!(assign_grade(-0.2, 0.5), ConfidenceGrade::Low);
    }

    // ─── Annualized Sharpe ───────────────────────────────────────

    #[test]
    fn annualized_sharpe_positive_returns() {
        // Positive daily returns with small variation: Sharpe should be very high
        let returns: Vec<f64> = (0..252)
            .map(|i| 0.001 + 0.0001 * ((i as f64 * 0.1).sin()))
            .collect();
        let sharpe = annualized_sharpe(&returns);
        assert!(sharpe > 10.0, "Expected very high Sharpe, got {sharpe}");
    }

    #[test]
    fn annualized_sharpe_zero_returns() {
        let returns = vec![0.0; 252];
        assert_eq!(annualized_sharpe(&returns), 0.0);
    }

    #[test]
    fn annualized_sharpe_mixed_returns() {
        // Alternating returns: lower Sharpe
        let returns: Vec<f64> = (0..252)
            .map(|i| if i % 2 == 0 { 0.01 } else { -0.005 })
            .collect();
        let sharpe = annualized_sharpe(&returns);
        assert!(sharpe > 0.0);
        assert!(sharpe < 10.0);
    }

    // ─── Block bootstrap ─────────────────────────────────────────

    #[test]
    fn bootstrap_insufficient_data() {
        let eq = vec![100.0; 100]; // < 252 returns
        let config = BootstrapConfig::default();
        let result = stationary_block_bootstrap(&eq, &config);
        assert!(result.is_err());
    }

    #[test]
    fn bootstrap_strong_positive() {
        // Strongly trending equity with realistic noise (~Sharpe 3)
        // Mean daily return ~0.08%, noise amplitude ~0.4% → realistic variance
        let mut eq = vec![100_000.0];
        for i in 1..=500 {
            let ret = 1.0 + 0.0008 + 0.004 * ((i as f64 * 0.3).sin());
            eq.push(eq[i - 1] * ret);
        }
        let config = BootstrapConfig {
            n_resamples: 1000,
            mean_block_length: 20,
            seed: 42,
        };
        let result = stationary_block_bootstrap(&eq, &config).unwrap();
        assert!(
            result.sharpe_ci_lower > 0.0,
            "CI lower should be positive for strong strategy, got {}",
            result.sharpe_ci_lower
        );
        assert!(
            result.grade == ConfidenceGrade::High || result.grade == ConfidenceGrade::Medium,
            "Expected High or Medium grade, got {:?} (ci_lower={}, ci_width={})",
            result.grade,
            result.sharpe_ci_lower,
            result.ci_width
        );
    }

    #[test]
    fn bootstrap_flat_equity() {
        // Flat equity → should get Low grade
        let eq = vec![100_000.0; 500];
        let config = BootstrapConfig {
            n_resamples: 500,
            mean_block_length: 20,
            seed: 42,
        };
        let result = stationary_block_bootstrap(&eq, &config).unwrap();
        assert_eq!(result.grade, ConfidenceGrade::Low);
    }

    #[test]
    fn bootstrap_deterministic() {
        let mut eq = vec![100_000.0];
        for i in 1..=300 {
            let daily = 1.0005 + 0.0001 * ((i as f64 * 0.1).sin());
            eq.push(eq[i - 1] * daily);
        }
        let config = BootstrapConfig {
            n_resamples: 200,
            mean_block_length: 20,
            seed: 123,
        };
        let r1 = stationary_block_bootstrap(&eq, &config).unwrap();
        let r2 = stationary_block_bootstrap(&eq, &config).unwrap();
        assert!((r1.sharpe_median - r2.sharpe_median).abs() < 1e-10);
    }

    // ─── Resample block ──────────────────────────────────────────

    #[test]
    fn resample_preserves_length() {
        let returns = vec![0.01; 100];
        let mut rng = StdRng::seed_from_u64(42);
        let resampled = resample_stationary_block(&returns, 100, 0.05, &mut rng);
        assert_eq!(resampled.len(), 100);
    }

    // ─── Common dates ────────────────────────────────────────────

    #[test]
    fn common_dates_intersection() {
        let d1 = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        let d4 = NaiveDate::from_ymd_opt(2024, 1, 4).unwrap();

        let mut dates = HashMap::new();
        dates.insert("A".into(), vec![d1, d2, d3]);
        dates.insert("B".into(), vec![d2, d3, d4]);

        let common = find_common_dates(&dates);
        assert_eq!(common.len(), 2);
        assert_eq!(common[0], d2);
        assert_eq!(common[1], d3);
    }

    #[test]
    fn common_dates_empty_intersection() {
        let d1 = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

        let mut dates = HashMap::new();
        dates.insert("A".into(), vec![d1]);
        dates.insert("B".into(), vec![d2]);

        let common = find_common_dates(&dates);
        assert!(common.is_empty());
    }

    // ─── Per-symbol diagnostic ───────────────────────────────────

    #[test]
    fn per_symbol_diagnostic_adequate() {
        let mut curves = HashMap::new();
        // Three symbols with positive returns (slight variation to avoid zero std)
        for (j, sym) in ["A", "B", "C"].iter().enumerate() {
            let mut eq = vec![100_000.0];
            for i in 1..=300 {
                let daily = 1.001 + 0.0002 * (((i + j * 100) as f64 * 0.1).sin());
                eq.push(eq[i - 1] * daily);
            }
            curves.insert(sym.to_string(), eq);
        }

        let diag = compute_per_symbol_diagnostic(&curves);
        assert_eq!(diag.symbol_count, 3);
        assert!(diag.mean_sharpe > 0.0);
        assert!(diag.hit_rate >= 0.3);
        assert!(diag.adequate);
    }

    #[test]
    fn per_symbol_diagnostic_inadequate_too_few() {
        let mut curves = HashMap::new();
        curves.insert("A".to_string(), vec![100_000.0; 300]);

        let diag = compute_per_symbol_diagnostic(&curves);
        assert!(!diag.adequate); // < 3 symbols
    }
}
