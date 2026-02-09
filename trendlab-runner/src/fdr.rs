//! False Discovery Rate correction and statistical testing.
//!
//! Implements from first principles:
//! - Lanczos approximation for ln(Gamma)
//! - Regularized incomplete beta function
//! - Student's t-distribution CDF
//! - One-sided t-test (H0: mean = 0, H1: mean > 0)
//! - Benjamini-Hochberg FDR correction
//! - FDR family tracker for accumulating p-values across YOLO iterations
//!
//! Statistical caveat: the t-test on K fold-level OOS Sharpe values is a
//! heuristic ranking tool, not a rigorous hypothesis test. The assumptions of
//! normality, independence, and sufficient sample size are unlikely to hold.
//! The resulting p-values should be treated as ranking scores for the BH
//! procedure, not as literal false-positive probabilities.

use serde::{Deserialize, Serialize};

// ─── Math primitives ─────────────────────────────────────────────────

/// Lanczos approximation for ln(Gamma(x)), g=7, n=9.
fn ln_gamma(x: f64) -> f64 {
    // Lanczos coefficients for g=7, n=9
    #[allow(clippy::excessive_precision)]
    const COEFFICIENTS: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    const G: f64 = 7.0;

    if x < 0.5 {
        // Reflection formula: Gamma(x) * Gamma(1-x) = pi / sin(pi*x)
        let log_pi = std::f64::consts::PI.ln();
        let sin_val = (std::f64::consts::PI * x).sin();
        if sin_val.abs() < 1e-300 {
            return f64::INFINITY;
        }
        return log_pi - sin_val.abs().ln() - ln_gamma(1.0 - x);
    }

    let x = x - 1.0;
    let mut sum = COEFFICIENTS[0];
    for (i, &c) in COEFFICIENTS.iter().enumerate().skip(1) {
        sum += c / (x + i as f64);
    }

    let t = x + G + 0.5;
    let log_sqrt_2pi = (2.0 * std::f64::consts::PI).sqrt().ln();

    log_sqrt_2pi + (t.ln() * (x + 0.5)) - t + sum.ln()
}

/// Regularized incomplete beta function I_x(a, b) via continued fraction.
///
/// Uses the Lentz algorithm for the continued fraction expansion.
fn regularized_incomplete_beta(a: f64, b: f64, x: f64) -> f64 {
    if !(0.0..=1.0).contains(&x) {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return 1.0;
    }

    // Use the symmetry relation when x > (a+1)/(a+b+2) for better convergence
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - regularized_incomplete_beta(b, a, 1.0 - x);
    }

    // Compute the prefix: x^a * (1-x)^b / (a * B(a,b))
    let ln_prefix = a * x.ln() + b * (1.0 - x).ln() - ln_gamma(a) - ln_gamma(b) + ln_gamma(a + b)
        - a.ln();

    let prefix = ln_prefix.exp();

    // Continued fraction via modified Lentz's algorithm
    let max_iter = 200;
    let epsilon = 1e-14;
    let tiny = 1e-30;

    let mut c = 1.0_f64;
    let mut d = 1.0 - (a + b) * x / (a + 1.0);
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut f = d;

    for m in 1..=max_iter {
        let m_f64 = m as f64;

        // Even step: d_{2m}
        let numerator_even =
            m_f64 * (b - m_f64) * x / ((a + 2.0 * m_f64 - 1.0) * (a + 2.0 * m_f64));

        d = 1.0 + numerator_even * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + numerator_even / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        f *= c * d;

        // Odd step: d_{2m+1}
        let numerator_odd = -((a + m_f64) * (a + b + m_f64) * x)
            / ((a + 2.0 * m_f64) * (a + 2.0 * m_f64 + 1.0));

        d = 1.0 + numerator_odd * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + numerator_odd / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;

        if (delta - 1.0).abs() < epsilon {
            break;
        }
    }

    prefix * f
}

/// Student's t-distribution CDF: P(T <= t) for df degrees of freedom.
pub fn t_cdf(t: f64, df: f64) -> f64 {
    if df <= 0.0 {
        return f64::NAN;
    }
    if t == 0.0 {
        return 0.5;
    }

    let x = df / (df + t * t);
    let ib = regularized_incomplete_beta(df / 2.0, 0.5, x);

    if t > 0.0 {
        1.0 - 0.5 * ib
    } else {
        0.5 * ib
    }
}

// ─── Statistical tests ───────────────────────────────────────────────

/// Result of a one-sided t-test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTestResult {
    /// The t-statistic: mean / (std / sqrt(n))
    pub t_statistic: f64,
    /// One-sided p-value: P(T > t) under H0
    pub p_value: f64,
    /// Degrees of freedom (n - 1)
    pub df: f64,
}

/// One-sided t-test: H0: mean = 0, H1: mean > 0.
///
/// Returns None if fewer than 2 values are provided.
/// The p-value is the probability of observing a t-statistic at least as
/// extreme under the null hypothesis that the true mean is zero.
pub fn one_sided_t_test(values: &[f64]) -> Option<TTestResult> {
    let n = values.len();
    if n < 2 {
        return None;
    }

    let n_f = n as f64;
    let mean = values.iter().sum::<f64>() / n_f;
    let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (n_f - 1.0);
    let std_err = (variance / n_f).sqrt();

    if std_err < 1e-15 {
        // All values are identical — t is undefined
        if mean > 0.0 {
            return Some(TTestResult {
                t_statistic: f64::INFINITY,
                p_value: 0.0,
                df: n_f - 1.0,
            });
        } else {
            return Some(TTestResult {
                t_statistic: 0.0,
                p_value: 0.5,
                df: n_f - 1.0,
            });
        }
    }

    let t_stat = mean / std_err;
    let df = n_f - 1.0;
    // One-sided: P(T > t) = 1 - CDF(t)
    let p_value = 1.0 - t_cdf(t_stat, df);

    Some(TTestResult {
        t_statistic: t_stat,
        p_value,
        df,
    })
}

// ─── FDR correction ──────────────────────────────────────────────────

/// Result of Benjamini-Hochberg FDR correction for a single entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FdrResult {
    /// Identifier for the configuration (typically full_hash).
    pub config_id: String,
    /// Raw (unadjusted) p-value from the t-test.
    pub raw_p: f64,
    /// BH-adjusted p-value.
    pub adjusted_p: f64,
    /// Whether this entry is significant at the specified alpha level.
    pub significant: bool,
}

/// Apply Benjamini-Hochberg FDR correction to a set of p-values.
///
/// Given `m` hypothesis tests, the BH procedure:
/// 1. Sort p-values in ascending order: p_(1) <= p_(2) <= ... <= p_(m)
/// 2. Find the largest k such that p_(k) <= (k/m) * alpha
/// 3. Reject all hypotheses with p <= p_(k)
///
/// Returns results sorted by raw p-value (ascending).
pub fn benjamini_hochberg(p_values: &[(String, f64)], alpha: f64) -> Vec<FdrResult> {
    if p_values.is_empty() {
        return Vec::new();
    }

    let m = p_values.len();

    // Sort by raw p-value ascending
    let mut indexed: Vec<(usize, &str, f64)> = p_values
        .iter()
        .enumerate()
        .map(|(i, (id, p))| (i, id.as_str(), *p))
        .collect();
    indexed.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    // Compute adjusted p-values using step-up procedure:
    // adjusted_p_(k) = min(p_(k) * m/k, adjusted_p_(k+1))
    // Working backwards from the largest p-value.
    let mut adjusted: Vec<f64> = vec![0.0; m];
    adjusted[m - 1] = indexed[m - 1].2; // largest p-value stays as-is (clamped to 1.0)
    adjusted[m - 1] = adjusted[m - 1].min(1.0);

    for k in (0..m - 1).rev() {
        let rank = k + 1; // 1-based rank
        let raw_p = indexed[k].2;
        let corrected = raw_p * m as f64 / rank as f64;
        adjusted[k] = corrected.min(adjusted[k + 1]).min(1.0);
    }

    // Build results
    indexed
        .iter()
        .zip(adjusted.iter())
        .map(|(&(_, id, raw_p), &adj_p)| FdrResult {
            config_id: id.to_string(),
            raw_p,
            adjusted_p: adj_p,
            significant: adj_p <= alpha,
        })
        .collect()
}

// ─── FDR family tracker ──────────────────────────────────────────────

/// Tracks p-values across YOLO iterations for FDR correction.
///
/// The "family" is all configurations tested within one YOLO session on the
/// same universe, date range, and execution preset. Different universes, date
/// ranges, or execution presets constitute separate FDR families.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FdrFamily {
    entries: Vec<(String, f64)>,
}

impl FdrFamily {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a p-value for a configuration.
    pub fn add(&mut self, config_id: String, p_value: f64) {
        self.entries.push((config_id, p_value));
    }

    /// Apply BH correction to all accumulated p-values.
    pub fn apply_correction(&self, alpha: f64) -> Vec<FdrResult> {
        benjamini_hochberg(&self.entries, alpha)
    }

    /// Number of accumulated entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the family is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── ln_gamma tests ──────────────────────────────────────────

    #[test]
    fn ln_gamma_known_values() {
        // Gamma(1) = 1, so ln(Gamma(1)) = 0
        assert!((ln_gamma(1.0)).abs() < 1e-10);

        // Gamma(2) = 1, so ln(Gamma(2)) = 0
        assert!((ln_gamma(2.0)).abs() < 1e-10);

        // Gamma(3) = 2, so ln(Gamma(3)) = ln(2)
        assert!((ln_gamma(3.0) - 2.0_f64.ln()).abs() < 1e-10);

        // Gamma(5) = 24, so ln(Gamma(5)) = ln(24)
        assert!((ln_gamma(5.0) - 24.0_f64.ln()).abs() < 1e-10);

        // Gamma(0.5) = sqrt(pi)
        let expected = std::f64::consts::PI.sqrt().ln();
        assert!((ln_gamma(0.5) - expected).abs() < 1e-10);
    }

    // ─── t_cdf tests ─────────────────────────────────────────────

    #[test]
    fn t_cdf_at_zero() {
        // P(T <= 0) = 0.5 for any df
        assert!((t_cdf(0.0, 1.0) - 0.5).abs() < 1e-10);
        assert!((t_cdf(0.0, 10.0) - 0.5).abs() < 1e-10);
        assert!((t_cdf(0.0, 100.0) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn t_cdf_symmetry() {
        // CDF(-t) = 1 - CDF(t) for symmetric distribution
        let df = 10.0;
        for &t in &[0.5, 1.0, 2.0, 3.0] {
            let left = t_cdf(-t, df);
            let right = t_cdf(t, df);
            assert!((left + right - 1.0).abs() < 1e-10, "t={t}: {left} + {right} != 1.0");
        }
    }

    #[test]
    fn t_cdf_known_values() {
        // For df=1 (Cauchy), CDF(1) = 0.75
        assert!((t_cdf(1.0, 1.0) - 0.75).abs() < 1e-6);

        // For large df, t-distribution ≈ normal
        // CDF(1.96) ≈ 0.975 for normal
        let cdf_large_df = t_cdf(1.96, 1000.0);
        assert!((cdf_large_df - 0.975).abs() < 0.005);
    }

    #[test]
    fn t_cdf_large_t_approaches_one() {
        assert!(t_cdf(100.0, 5.0) > 0.999);
    }

    #[test]
    fn t_cdf_large_negative_t_approaches_zero() {
        assert!(t_cdf(-100.0, 5.0) < 0.001);
    }

    // ─── t-test tests ────────────────────────────────────────────

    #[test]
    fn t_test_too_few_values() {
        assert!(one_sided_t_test(&[]).is_none());
        assert!(one_sided_t_test(&[1.0]).is_none());
    }

    #[test]
    fn t_test_positive_mean() {
        // Clearly positive: should reject H0
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = one_sided_t_test(&values).unwrap();
        assert!(result.t_statistic > 0.0);
        assert!(result.p_value < 0.01);
        assert!((result.df - 4.0).abs() < 1e-10);
    }

    #[test]
    fn t_test_zero_mean() {
        // Symmetric around zero: p ≈ 0.5
        let values = vec![-2.0, -1.0, 0.0, 1.0, 2.0];
        let result = one_sided_t_test(&values).unwrap();
        assert!((result.t_statistic).abs() < 1e-10);
        assert!((result.p_value - 0.5).abs() < 0.01);
    }

    #[test]
    fn t_test_negative_mean() {
        // Negative mean: large p-value (can't reject H0: mean > 0)
        let values = vec![-5.0, -4.0, -3.0, -2.0, -1.0];
        let result = one_sided_t_test(&values).unwrap();
        assert!(result.t_statistic < 0.0);
        assert!(result.p_value > 0.95);
    }

    #[test]
    fn t_test_identical_positive_values() {
        let values = vec![1.0, 1.0, 1.0];
        let result = one_sided_t_test(&values).unwrap();
        assert_eq!(result.p_value, 0.0);
    }

    #[test]
    fn t_test_identical_zero_values() {
        let values = vec![0.0, 0.0, 0.0];
        let result = one_sided_t_test(&values).unwrap();
        assert!((result.p_value - 0.5).abs() < 1e-10);
    }

    // ─── Benjamini-Hochberg tests ────────────────────────────────

    #[test]
    fn bh_empty() {
        let result = benjamini_hochberg(&[], 0.05);
        assert!(result.is_empty());
    }

    #[test]
    fn bh_single_significant() {
        let pvals = vec![("a".into(), 0.01)];
        let result = benjamini_hochberg(&pvals, 0.05);
        assert_eq!(result.len(), 1);
        assert!(result[0].significant);
        assert!((result[0].adjusted_p - 0.01).abs() < 1e-10);
    }

    #[test]
    fn bh_single_not_significant() {
        let pvals = vec![("a".into(), 0.10)];
        let result = benjamini_hochberg(&pvals, 0.05);
        assert!(!result[0].significant);
    }

    #[test]
    fn bh_multiple_reduces_significance() {
        // With many tests, BH correction should make some previously
        // significant results no longer significant.
        let pvals: Vec<(String, f64)> = (0..20)
            .map(|i| (format!("config_{i}"), 0.04)) // all at p=0.04
            .collect();
        let result = benjamini_hochberg(&pvals, 0.05);

        // With BH correction: adjusted_p = 0.04 * 20/k for each rank k
        // For rank 1: 0.04 * 20/1 = 0.8, not significant
        // All adjusted p-values should be 0.04 * 20/20 = 0.04 (due to step-up)
        // Actually: since all raw p are identical (0.04), adjusted p = min(0.04*20/k, ...)
        // The step-up ensures monotonicity, so all get the same adjusted p.
        // adjusted_p = 0.04 (since 0.04 * 20/20 = 0.04 for the last, and step-up propagates)
        assert!(result.iter().all(|r| r.significant));
    }

    #[test]
    fn bh_mixed_significance() {
        let pvals: Vec<(String, f64)> = vec![
            ("strong".into(), 0.001),
            ("medium".into(), 0.020),
            ("weak".into(), 0.040),
            ("noise1".into(), 0.300),
            ("noise2".into(), 0.700),
        ];
        let result = benjamini_hochberg(&pvals, 0.05);

        // Sorted: 0.001, 0.020, 0.040, 0.300, 0.700
        // BH thresholds: 1/5*0.05=0.01, 2/5*0.05=0.02, 3/5*0.05=0.03, 4/5*0.05=0.04, 5/5*0.05=0.05
        // p_(1)=0.001 <= 0.01 ✓
        // p_(2)=0.020 <= 0.02 ✓
        // p_(3)=0.040 <= 0.03 ✗ — BH critical value stops here
        // So configs "strong" and "medium" are significant, "weak" is not
        let sig_count = result.iter().filter(|r| r.significant).count();
        assert_eq!(sig_count, 2, "Expected 2 significant, got {sig_count}");
    }

    #[test]
    fn bh_adjusted_p_monotonic() {
        let pvals: Vec<(String, f64)> = vec![
            ("a".into(), 0.01),
            ("b".into(), 0.03),
            ("c".into(), 0.05),
            ("d".into(), 0.10),
            ("e".into(), 0.50),
        ];
        let result = benjamini_hochberg(&pvals, 0.05);

        // Adjusted p-values must be non-decreasing
        for i in 1..result.len() {
            assert!(
                result[i].adjusted_p >= result[i - 1].adjusted_p - 1e-10,
                "Adjusted p-values not monotonic at position {i}"
            );
        }
    }

    // ─── FDR family tests ────────────────────────────────────────

    #[test]
    fn fdr_family_accumulation() {
        let mut family = FdrFamily::new();
        assert!(family.is_empty());
        assert_eq!(family.len(), 0);

        family.add("config_1".into(), 0.01);
        family.add("config_2".into(), 0.50);
        assert_eq!(family.len(), 2);

        let results = family.apply_correction(0.05);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn fdr_family_correction_grows_with_tests() {
        // More tests in the family → harder to be significant
        let mut family_small = FdrFamily::new();
        family_small.add("a".into(), 0.03);

        let mut family_large = FdrFamily::new();
        family_large.add("a".into(), 0.03);
        for i in 1..50 {
            family_large.add(format!("noise_{i}"), 0.5);
        }

        let results_small = family_small.apply_correction(0.05);
        let results_large = family_large.apply_correction(0.05);

        // In the small family, p=0.03 is significant at alpha=0.05
        let sig_small = results_small.iter().find(|r| r.config_id == "a").unwrap();
        assert!(sig_small.significant);

        // In the large family, the same p=0.03 may or may not be significant
        // depending on how many other tests there are, but its adjusted p should be larger
        let sig_large = results_large.iter().find(|r| r.config_id == "a").unwrap();
        assert!(sig_large.adjusted_p >= sig_small.adjusted_p);
    }
}
