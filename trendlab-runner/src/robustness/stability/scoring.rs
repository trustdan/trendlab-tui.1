//! Stability score calculation.
//!
//! Penalizes variance: score = median - penalty_factor * IQR
//! This rewards consistent strategies over high-but-unstable ones.

use serde::{Deserialize, Serialize};
use super::percentile;

/// Stability score for a metric across multiple trials.
///
/// Formula: `score = median - penalty_factor * IQR`
/// where IQR = Q3 - Q1 (interquartile range).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StabilityScore {
    /// Metric name (e.g., "sharpe", "sortino")
    pub metric: String,
    /// Median value across trials
    pub median: f64,
    /// Interquartile range (Q3 - Q1)
    pub iqr: f64,
    /// Stability score (median - penalty * IQR)
    pub score: f64,
    /// Penalty factor applied to IQR
    pub penalty_factor: f64,
}

impl StabilityScore {
    /// Compute stability score from a set of metric values.
    ///
    /// # Arguments
    /// * `metric` - Name of the metric being scored
    /// * `values` - Array of metric values from multiple trials
    /// * `penalty` - Penalty factor for variance (typically 0.5)
    ///
    /// # Example
    /// ```
    /// use trendlab_runner::robustness::StabilityScore;
    ///
    /// let sharpe_trials = vec![1.8, 2.0, 1.9, 2.1, 1.85];
    /// let score = StabilityScore::compute("sharpe", &sharpe_trials, 0.5);
    ///
    /// // Low IQR = stable strategy = high score
    /// assert!(score.score > 1.8);
    /// ```
    pub fn compute(metric: &str, values: &[f64], penalty: f64) -> Self {
        if values.is_empty() {
            return Self {
                metric: metric.to_string(),
                median: 0.0,
                iqr: 0.0,
                score: 0.0,
                penalty_factor: penalty,
            };
        }

        let median = percentile(values, 0.5);
        let q1 = percentile(values, 0.25);
        let q3 = percentile(values, 0.75);
        let iqr = q3 - q1;
        let score = median - (penalty * iqr);

        Self {
            metric: metric.to_string(),
            median,
            iqr,
            score,
            penalty_factor: penalty,
        }
    }

    /// Returns true if the score meets the minimum threshold.
    pub fn meets_threshold(&self, min_score: f64) -> bool {
        self.score >= min_score
    }

    /// Returns true if the IQR is below the maximum allowed variance.
    pub fn is_stable(&self, max_iqr: f64) -> bool {
        self.iqr <= max_iqr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stability_score_low_variance() {
        let values = vec![1.8, 1.9, 2.0, 1.9, 2.1];
        let score = StabilityScore::compute("sharpe", &values, 0.5);

        assert_eq!(score.metric, "sharpe");
        assert!((score.median - 1.9).abs() < 0.01);
        assert!(score.iqr < 0.3); // Low variance
        assert!(score.score > 1.8); // High stability score
    }

    #[test]
    fn test_stability_score_high_variance() {
        let values = vec![0.5, 2.5, 1.0, 3.0, 1.5];
        let score = StabilityScore::compute("sharpe", &values, 0.5);

        assert_eq!(score.metric, "sharpe");
        assert!((score.median - 1.5).abs() < 0.01);
        assert!(score.iqr > 1.0); // High variance
        assert!(score.score < 1.5); // Penalized for instability
    }

    #[test]
    fn test_meets_threshold() {
        let values = vec![2.0, 2.1, 1.9, 2.0, 2.05];
        let score = StabilityScore::compute("sharpe", &values, 0.5);

        assert!(score.meets_threshold(1.5));
        assert!(!score.meets_threshold(2.5));
    }

    #[test]
    fn test_is_stable() {
        let low_variance = vec![2.0, 2.1, 1.9, 2.0, 2.05];
        let high_variance = vec![0.5, 3.0, 1.0, 2.5, 1.5];

        let stable = StabilityScore::compute("sharpe", &low_variance, 0.5);
        let unstable = StabilityScore::compute("sharpe", &high_variance, 0.5);

        assert!(stable.is_stable(0.5));
        assert!(!unstable.is_stable(0.5));
    }

    #[test]
    fn test_empty_values() {
        let score = StabilityScore::compute("sharpe", &[], 0.5);
        assert_eq!(score.median, 0.0);
        assert_eq!(score.iqr, 0.0);
        assert_eq!(score.score, 0.0);
    }
}
