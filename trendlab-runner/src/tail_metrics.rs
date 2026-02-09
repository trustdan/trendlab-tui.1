//! Tail risk metrics — CVaR, skewness, kurtosis, downside deviation ratio.
//!
//! These complement the core `PerformanceMetrics` with distribution shape
//! statistics needed by the cross-symbol leaderboard and risk profile system.
//! All functions are pure: daily returns in, scalar out.

use serde::{Deserialize, Serialize};

use crate::metrics::{daily_returns, mean_f64, std_dev};

/// Minimum number of daily return observations required for tail metrics.
/// Below this threshold, all Optional fields return None.
pub const MIN_RETURN_OBSERVATIONS: usize = 252;

/// Tail risk statistics computed from an equity curve's daily returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailMetrics {
    /// Conditional Value at Risk at 95% — average loss in the worst 5% of days.
    /// Expressed as a negative number (e.g., -0.025 means average 2.5% daily loss
    /// in the worst 5% of days).
    pub cvar_95: Option<f64>,

    /// Skewness of daily returns (third standardized moment).
    /// Negative = left tail heavier (more large losses than gains).
    pub skewness: Option<f64>,

    /// Excess kurtosis of daily returns (fourth standardized moment - 3).
    /// Positive = heavier tails than normal distribution.
    pub kurtosis: Option<f64>,

    /// Downside deviation ratio: downside_std / total_std.
    /// Values > 1.0 indicate asymmetrically large downside moves.
    pub downside_deviation_ratio: Option<f64>,

    /// Number of return observations used.
    pub sample_size: usize,
}

/// Compute all tail risk metrics from an equity curve.
///
/// Returns `TailMetrics` with `None` for all statistical fields if the equity
/// curve has fewer than `MIN_RETURN_OBSERVATIONS + 1` data points (we need
/// n+1 prices to compute n returns).
pub fn compute_tail_metrics(equity_curve: &[f64]) -> TailMetrics {
    let returns = daily_returns(equity_curve);
    let n = returns.len();

    if n < MIN_RETURN_OBSERVATIONS {
        return TailMetrics {
            cvar_95: None,
            skewness: None,
            kurtosis: None,
            downside_deviation_ratio: None,
            sample_size: n,
        };
    }

    TailMetrics {
        cvar_95: Some(cvar_95(&returns)),
        skewness: Some(skewness(&returns)),
        kurtosis: Some(excess_kurtosis(&returns)),
        downside_deviation_ratio: Some(downside_deviation_ratio(&returns)),
        sample_size: n,
    }
}

/// Conditional Value at Risk at the 95th percentile.
///
/// Sort returns ascending, take the bottom 5%, and compute their mean.
/// This represents the expected loss on the worst 5% of trading days.
fn cvar_95(returns: &[f64]) -> f64 {
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let cutoff = (sorted.len() as f64 * 0.05).ceil() as usize;
    let cutoff = cutoff.max(1); // at least 1 observation
    let tail = &sorted[..cutoff];

    mean_f64(tail)
}

/// Skewness (third standardized moment).
///
/// skew = (1/n) * sum((x_i - mean)^3) / std^3
///
/// Uses the population formula (not sample-adjusted) for consistency
/// with our Sharpe/Sortino calculations.
fn skewness(returns: &[f64]) -> f64 {
    let n = returns.len() as f64;
    let mean = mean_f64(returns);
    let std = std_dev(returns);

    if std < 1e-15 {
        return 0.0;
    }

    let m3 = returns
        .iter()
        .map(|r| ((r - mean) / std).powi(3))
        .sum::<f64>()
        / n;
    m3
}

/// Excess kurtosis (fourth standardized moment minus 3).
///
/// kurt = (1/n) * sum((x_i - mean)^4) / std^4 - 3
///
/// Excess kurtosis: 0 for normal distribution, positive for fat tails.
fn excess_kurtosis(returns: &[f64]) -> f64 {
    let n = returns.len() as f64;
    let mean = mean_f64(returns);
    let std = std_dev(returns);

    if std < 1e-15 {
        return 0.0;
    }

    let m4 = returns
        .iter()
        .map(|r| ((r - mean) / std).powi(4))
        .sum::<f64>()
        / n;
    m4 - 3.0
}

/// Downside deviation ratio: downside_std / total_std.
///
/// Downside deviation considers only returns below zero (or below the target
/// return, here zero). Ratio > 1.0 means downside moves are disproportionately
/// large relative to overall volatility.
fn downside_deviation_ratio(returns: &[f64]) -> f64 {
    let total_std = std_dev(returns);
    if total_std < 1e-15 {
        return 0.0;
    }

    let n = returns.len() as f64;
    let downside_sq_sum: f64 = returns.iter().filter(|&&r| r < 0.0).map(|r| r * r).sum();

    // Use full n in denominator (not just count of negatives) for consistency
    // with the Sortino ratio convention.
    let downside_var = downside_sq_sum / n;
    let downside_std = downside_var.sqrt();

    downside_std / total_std
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an equity curve from daily returns.
    fn equity_from_returns(initial: f64, returns: &[f64]) -> Vec<f64> {
        let mut curve = Vec::with_capacity(returns.len() + 1);
        curve.push(initial);
        for &r in returns {
            curve.push(curve.last().unwrap() * (1.0 + r));
        }
        curve
    }

    #[test]
    fn insufficient_data_returns_none() {
        // 100 bars = 99 returns, below 252 threshold
        let eq = vec![100_000.0; 100];
        let tm = compute_tail_metrics(&eq);
        assert!(tm.cvar_95.is_none());
        assert!(tm.skewness.is_none());
        assert!(tm.kurtosis.is_none());
        assert!(tm.downside_deviation_ratio.is_none());
        assert_eq!(tm.sample_size, 99);
    }

    #[test]
    fn empty_curve_returns_none() {
        let tm = compute_tail_metrics(&[]);
        assert!(tm.cvar_95.is_none());
        assert_eq!(tm.sample_size, 0);
    }

    #[test]
    fn single_price_returns_none() {
        let tm = compute_tail_metrics(&[100_000.0]);
        assert!(tm.cvar_95.is_none());
        assert_eq!(tm.sample_size, 0);
    }

    #[test]
    fn sufficient_data_returns_some() {
        // 253 prices = 252 returns, meets threshold
        let returns: Vec<f64> = (0..252)
            .map(|i| if i % 2 == 0 { 0.001 } else { -0.0005 })
            .collect();
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        assert!(tm.cvar_95.is_some());
        assert!(tm.skewness.is_some());
        assert!(tm.kurtosis.is_some());
        assert!(tm.downside_deviation_ratio.is_some());
        assert_eq!(tm.sample_size, 252);
    }

    #[test]
    fn cvar_95_is_negative_for_mixed_returns() {
        let returns: Vec<f64> = (0..300)
            .map(|i| if i % 3 == 0 { -0.02 } else { 0.005 })
            .collect();
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        // CVaR should be negative (worst 5% of days are losses)
        let cvar = tm.cvar_95.unwrap();
        assert!(cvar < 0.0, "CVaR should be negative, got {cvar}");
    }

    #[test]
    fn skewness_negative_for_left_skewed() {
        // More large negative returns than positive ones
        let mut returns: Vec<f64> = vec![0.001; 252];
        // Inject large drops
        for i in (0..252).step_by(20) {
            returns[i] = -0.05;
        }
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        let skew = tm.skewness.unwrap();
        assert!(
            skew < 0.0,
            "Skewness should be negative for left-skewed, got {skew}"
        );
    }

    #[test]
    fn kurtosis_positive_for_fat_tails() {
        // Mix of normal days with occasional extreme moves → excess kurtosis > 0
        let mut returns: Vec<f64> = vec![0.001; 300];
        for i in (0..300).step_by(15) {
            returns[i] = if i % 30 == 0 { 0.08 } else { -0.06 };
        }
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        let kurt = tm.kurtosis.unwrap();
        assert!(
            kurt > 0.0,
            "Excess kurtosis should be positive for fat tails, got {kurt}"
        );
    }

    #[test]
    fn downside_deviation_ratio_reasonable() {
        let returns: Vec<f64> = (0..300)
            .map(|i| if i % 2 == 0 { 0.002 } else { -0.001 })
            .collect();
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        let ddr = tm.downside_deviation_ratio.unwrap();
        assert!(ddr > 0.0, "DDR should be positive, got {ddr}");
        assert!(ddr < 2.0, "DDR should be reasonable, got {ddr}");
    }

    #[test]
    fn constant_returns_zero_metrics() {
        // Constant returns (zero std) → all metrics 0
        let returns = vec![0.001; 300];
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        // Constant returns → zero std → skewness/kurtosis/DDR = 0
        // But CVaR is well-defined (average of bottom 5% = 0.001)
        assert_eq!(tm.skewness.unwrap(), 0.0);
        assert_eq!(tm.kurtosis.unwrap(), 0.0);
        assert_eq!(tm.downside_deviation_ratio.unwrap(), 0.0);
    }

    #[test]
    fn all_positive_returns_cvar_still_defined() {
        let returns = vec![0.005; 300];
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);

        // All positive → CVaR is the mean of the smallest 5%, which is still 0.005
        let cvar = tm.cvar_95.unwrap();
        assert!((cvar - 0.005).abs() < 1e-10);
    }

    #[test]
    fn all_negative_constant_returns_zero_ddr() {
        // Constant returns (even if negative) have zero std → DDR = 0.0
        let returns = vec![-0.005; 300];
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);
        let ddr = tm.downside_deviation_ratio.unwrap();
        assert_eq!(ddr, 0.0);
    }

    #[test]
    fn mostly_negative_returns_high_ddr() {
        // Alternating -0.02 and +0.001 → most variance is on the downside
        let returns: Vec<f64> = (0..300)
            .map(|i| if i % 3 == 0 { 0.001 } else { -0.01 })
            .collect();
        let eq = equity_from_returns(100_000.0, &returns);
        let tm = compute_tail_metrics(&eq);
        let ddr = tm.downside_deviation_ratio.unwrap();
        assert!(
            ddr > 0.5,
            "DDR should be high for mostly negative returns, got {ddr}"
        );
    }

    #[test]
    fn tail_metrics_serialization_roundtrip() {
        let tm = TailMetrics {
            cvar_95: Some(-0.02),
            skewness: Some(-0.5),
            kurtosis: Some(1.2),
            downside_deviation_ratio: Some(0.8),
            sample_size: 300,
        };
        let json = serde_json::to_string(&tm).unwrap();
        let deser: TailMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.sample_size, 300);
        assert!((deser.cvar_95.unwrap() - (-0.02)).abs() < 1e-10);
    }
}
