//! Promotion filtering and criteria for the robustness ladder.

use super::stability::StabilityScore;

/// Criteria for promoting a candidate to the next level.
#[derive(Debug, Clone, PartialEq)]
pub struct PromotionCriteria {
    /// Minimum stability score required
    pub min_stability_score: f64,
    /// Maximum IQR allowed (variance threshold)
    pub max_iqr: f64,
    /// Minimum number of trades required
    pub min_trades: Option<usize>,
    /// Minimum raw metric value (e.g., sharpe > 0.5)
    pub min_raw_metric: Option<f64>,
}

impl PromotionCriteria {
    /// Create default promotion criteria.
    pub fn default_for_level(level: usize) -> Self {
        match level {
            1 => Self {
                // Cheap Pass: lenient, just filter obvious losers
                min_stability_score: 0.5,
                max_iqr: f64::INFINITY,
                min_trades: Some(5),
                min_raw_metric: Some(0.5),
            },
            2 => Self {
                // Walk-Forward: require OOS performance
                min_stability_score: 0.8,
                max_iqr: 1.0,
                min_trades: Some(10),
                min_raw_metric: Some(0.8),
            },
            3 => Self {
                // Execution MC: require stability
                min_stability_score: 1.5,
                max_iqr: 0.5,
                min_trades: Some(10),
                min_raw_metric: Some(1.0),
            },
            _ => Self {
                // Higher levels: very strict
                min_stability_score: 2.0,
                max_iqr: 0.3,
                min_trades: Some(10),
                min_raw_metric: Some(1.2),
            },
        }
    }
}

/// Filter that decides whether a candidate promotes to the next level.
///
/// # Purpose
/// Gate expensive validation behind cheap tests to save compute budget.
///
/// # Example
/// ```
/// use trendlab_runner::robustness::{PromotionFilter, PromotionCriteria, StabilityScore};
///
/// let criteria = PromotionCriteria {
///     min_stability_score: 1.5,
///     max_iqr: 0.5,
///     min_trades: Some(10),
///     min_raw_metric: Some(1.0),
/// };
/// let filter = PromotionFilter::new(criteria);
///
/// let stable = StabilityScore::compute("sharpe", &[1.9, 2.0, 2.1, 1.95, 2.05], 0.5);
/// assert!(filter.should_promote(&stable, 15, 2.0));
///
/// let unstable = StabilityScore::compute("sharpe", &[0.5, 3.0, 1.0, 2.5, 1.5], 0.5);
/// assert!(!filter.should_promote(&unstable, 15, 2.0));
/// ```
#[derive(Debug, Clone)]
pub struct PromotionFilter {
    pub criteria: PromotionCriteria,
}

impl PromotionFilter {
    /// Create a new promotion filter.
    pub fn new(criteria: PromotionCriteria) -> Self {
        Self { criteria }
    }

    /// Decide if a candidate should promote to the next level.
    ///
    /// # Arguments
    /// * `stability` - Stability score for the primary metric
    /// * `trade_count` - Number of trades executed
    /// * `raw_metric` - Raw metric value (e.g., median sharpe)
    pub fn should_promote(
        &self,
        stability: &StabilityScore,
        trade_count: usize,
        raw_metric: f64,
    ) -> bool {
        // Check stability score threshold
        if !stability.meets_threshold(self.criteria.min_stability_score) {
            return false;
        }

        // Check variance threshold
        if !stability.is_stable(self.criteria.max_iqr) {
            return false;
        }

        // Check minimum trades
        if let Some(min_trades) = self.criteria.min_trades {
            if trade_count < min_trades {
                return false;
            }
        }

        // Check raw metric threshold
        if let Some(min_raw) = self.criteria.min_raw_metric {
            if raw_metric < min_raw {
                return false;
            }
        }

        true
    }

    /// Get rejection reason if candidate fails to promote.
    pub fn rejection_reason(
        &self,
        stability: &StabilityScore,
        trade_count: usize,
        raw_metric: f64,
    ) -> Option<String> {
        if !stability.meets_threshold(self.criteria.min_stability_score) {
            return Some(format!(
                "StabilityScore too low: {:.2} < {:.2}",
                stability.score, self.criteria.min_stability_score
            ));
        }

        if !stability.is_stable(self.criteria.max_iqr) {
            return Some(format!(
                "IQR too high (unstable): {:.2} > {:.2}",
                stability.iqr, self.criteria.max_iqr
            ));
        }

        if let Some(min_trades) = self.criteria.min_trades {
            if trade_count < min_trades {
                return Some(format!(
                    "Insufficient trades: {} < {}",
                    trade_count, min_trades
                ));
            }
        }

        if let Some(min_raw) = self.criteria.min_raw_metric {
            if raw_metric < min_raw {
                return Some(format!(
                    "Raw metric too low: {:.2} < {:.2}",
                    raw_metric, min_raw
                ));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotion_filter_accepts_stable() {
        let criteria = PromotionCriteria {
            min_stability_score: 1.5,
            max_iqr: 0.5,
            min_trades: Some(10),
            min_raw_metric: Some(1.0),
        };
        let filter = PromotionFilter::new(criteria);

        let stable = StabilityScore::compute("sharpe", &[1.9, 2.0, 2.1, 1.95, 2.05], 0.5);
        assert!(filter.should_promote(&stable, 15, 2.0));
        assert!(filter.rejection_reason(&stable, 15, 2.0).is_none());
    }

    #[test]
    fn test_promotion_filter_rejects_unstable() {
        let criteria = PromotionCriteria {
            min_stability_score: 1.5,
            max_iqr: 0.5,
            min_trades: Some(10),
            min_raw_metric: Some(1.0),
        };
        let filter = PromotionFilter::new(criteria);

        let unstable = StabilityScore::compute("sharpe", &[0.5, 3.0, 1.0, 2.5, 1.5], 0.5);
        assert!(!filter.should_promote(&unstable, 15, 2.0));
        assert!(filter.rejection_reason(&unstable, 15, 2.0).is_some());
    }

    #[test]
    fn test_promotion_filter_rejects_few_trades() {
        let criteria = PromotionCriteria {
            min_stability_score: 1.0,
            max_iqr: 1.0,
            min_trades: Some(10),
            min_raw_metric: None,
        };
        let filter = PromotionFilter::new(criteria);

        let stable = StabilityScore::compute("sharpe", &[2.0, 2.0, 2.0], 0.5);
        assert!(!filter.should_promote(&stable, 5, 2.0)); // Too few trades
        assert!(filter.should_promote(&stable, 15, 2.0)); // Enough trades
    }

    #[test]
    fn test_promotion_filter_rejects_low_metric() {
        let criteria = PromotionCriteria {
            min_stability_score: 1.0,
            max_iqr: 1.0,
            min_trades: None,
            min_raw_metric: Some(1.5),
        };
        let filter = PromotionFilter::new(criteria);

        let stable = StabilityScore::compute("sharpe", &[1.0, 1.1, 1.05], 0.5);
        assert!(!filter.should_promote(&stable, 10, 1.05)); // Metric too low
        assert!(filter.should_promote(&stable, 10, 1.8)); // Metric high enough
    }

    #[test]
    fn test_default_criteria_progression() {
        let level1 = PromotionCriteria::default_for_level(1);
        let level2 = PromotionCriteria::default_for_level(2);
        let level3 = PromotionCriteria::default_for_level(3);

        // Criteria should get stricter at higher levels
        assert!(level1.min_stability_score < level2.min_stability_score);
        assert!(level2.min_stability_score < level3.min_stability_score);

        // Level3 should have lower max_iqr (stricter variance requirement)
        assert!(level3.max_iqr < level2.max_iqr);
    }
}
