//! Storage for metric distributions across multiple trials.
//!
//! Unlike point estimates, distributions preserve uncertainty information
//! for downstream analysis and visualization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::percentile;

/// Full distribution of a metric across multiple trials.
///
/// Stores both summary statistics and raw values for:
/// - Uncertainty quantification
/// - Distribution visualization
/// - Post-hoc analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricDistribution {
    /// Metric name
    pub metric: String,
    /// Median (50th percentile)
    pub median: f64,
    /// Mean (average)
    pub mean: f64,
    /// Interquartile range (Q3 - Q1)
    pub iqr: f64,
    /// Percentiles (p10, p25, p75, p90, etc.)
    pub percentiles: HashMap<String, f64>,
    /// All trial values (for full distribution)
    pub all_values: Vec<f64>,
}

impl MetricDistribution {
    /// Create a distribution from trial values.
    ///
    /// # Example
    /// ```
    /// use trendlab_runner::robustness::MetricDistribution;
    ///
    /// let sharpe_trials = vec![1.8, 2.0, 1.9, 2.1, 1.85];
    /// let dist = MetricDistribution::from_values("sharpe", &sharpe_trials);
    ///
    /// assert_eq!(dist.metric, "sharpe");
    /// assert!(dist.median > 1.8 && dist.median < 2.1);
    /// ```
    pub fn from_values(metric: &str, values: &[f64]) -> Self {
        if values.is_empty() {
            return Self {
                metric: metric.to_string(),
                median: 0.0,
                mean: 0.0,
                iqr: 0.0,
                percentiles: HashMap::new(),
                all_values: vec![],
            };
        }

        let median = percentile(values, 0.5);
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let q1 = percentile(values, 0.25);
        let q3 = percentile(values, 0.75);
        let iqr = q3 - q1;

        let mut percentiles = HashMap::new();
        percentiles.insert("p2.5".to_string(), percentile(values, 0.025));
        percentiles.insert("p10".to_string(), percentile(values, 0.10));
        percentiles.insert("p25".to_string(), q1);
        percentiles.insert("p50".to_string(), median);
        percentiles.insert("p75".to_string(), q3);
        percentiles.insert("p90".to_string(), percentile(values, 0.90));
        percentiles.insert("p97.5".to_string(), percentile(values, 0.975));

        Self {
            metric: metric.to_string(),
            median,
            mean,
            iqr,
            percentiles,
            all_values: values.to_vec(),
        }
    }

    /// Get a specific percentile value.
    pub fn get_percentile(&self, p: &str) -> Option<f64> {
        self.percentiles.get(p).copied()
    }

    /// Calculate standard deviation of the distribution.
    pub fn std_dev(&self) -> f64 {
        if self.all_values.is_empty() {
            return 0.0;
        }

        let variance = self
            .all_values
            .iter()
            .map(|v| (v - self.mean).powi(2))
            .sum::<f64>()
            / self.all_values.len() as f64;

        variance.sqrt()
    }

    /// Return the 95% confidence interval (p2.5, p97.5).
    ///
    /// Returns `None` if the distribution has fewer than 2 values.
    pub fn ci_95(&self) -> Option<(f64, f64)> {
        if self.all_values.len() < 2 {
            return None;
        }
        let lower = self.percentiles.get("p2.5").copied()?;
        let upper = self.percentiles.get("p97.5").copied()?;
        Some((lower, upper))
    }

    /// Return (min, max) values.
    pub fn range(&self) -> (f64, f64) {
        if self.all_values.is_empty() {
            return (0.0, 0.0);
        }

        let mut values = self.all_values.clone();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        (*values.first().unwrap(), *values.last().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distribution_from_values() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let dist = MetricDistribution::from_values("sharpe", &values);

        assert_eq!(dist.metric, "sharpe");
        assert_eq!(dist.median, 3.0);
        assert_eq!(dist.mean, 3.0);
        assert_eq!(dist.all_values.len(), 5);
    }

    #[test]
    fn test_get_percentile() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let dist = MetricDistribution::from_values("sharpe", &values);

        assert_eq!(dist.get_percentile("p50"), Some(3.0));
        assert!(dist.get_percentile("p90").is_some());
        assert!(dist.get_percentile("p99").is_none());
    }

    #[test]
    fn test_std_dev() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let dist = MetricDistribution::from_values("sharpe", &values);

        let std_dev = dist.std_dev();
        assert!((std_dev - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_range() {
        let values = vec![1.5, 2.0, 3.5, 4.0, 5.5];
        let dist = MetricDistribution::from_values("sharpe", &values);

        let (min, max) = dist.range();
        assert_eq!(min, 1.5);
        assert_eq!(max, 5.5);
    }

    #[test]
    fn test_ci_95() {
        let values: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let dist = MetricDistribution::from_values("sharpe", &values);

        let ci = dist.ci_95();
        assert!(ci.is_some());
        let (lower, upper) = ci.unwrap();
        assert!(lower < upper, "CI lower ({}) should be < upper ({})", lower, upper);
        assert!(lower < 5.0, "p2.5 should be near the low end");
        assert!(upper > 95.0, "p97.5 should be near the high end");
    }

    #[test]
    fn test_ci_95_insufficient_data() {
        let dist = MetricDistribution::from_values("sharpe", &[1.0]);
        assert!(dist.ci_95().is_none());

        let dist_empty = MetricDistribution::from_values("sharpe", &[]);
        assert!(dist_empty.ci_95().is_none());
    }

    #[test]
    fn test_empty_distribution() {
        let dist = MetricDistribution::from_values("sharpe", &[]);

        assert_eq!(dist.median, 0.0);
        assert_eq!(dist.mean, 0.0);
        assert_eq!(dist.std_dev(), 0.0);
        assert_eq!(dist.range(), (0.0, 0.0));
    }
}
