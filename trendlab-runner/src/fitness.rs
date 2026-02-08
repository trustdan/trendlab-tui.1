//! Fitness function — configurable metric selector for strategy ranking.

use crate::metrics::PerformanceMetrics;
use serde::{Deserialize, Serialize};

/// Which metric to optimize/sort by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitnessMetric {
    #[default]
    Sharpe,
    Sortino,
    Calmar,
    Cagr,
    WinRate,
    ProfitFactor,
    MaxDrawdown,
}

impl FitnessMetric {
    /// Extract the relevant metric value from a PerformanceMetrics struct.
    pub fn extract(&self, metrics: &PerformanceMetrics) -> f64 {
        match self {
            Self::Sharpe => metrics.sharpe,
            Self::Sortino => metrics.sortino,
            Self::Calmar => metrics.calmar,
            Self::Cagr => metrics.cagr,
            Self::WinRate => metrics.win_rate,
            Self::ProfitFactor => metrics.profit_factor,
            Self::MaxDrawdown => metrics.max_drawdown,
        }
    }

    /// Whether higher values are better for this metric.
    ///
    /// All metrics except MaxDrawdown: higher is better.
    /// MaxDrawdown: less negative (closer to 0) is better.
    pub fn is_higher_better(&self) -> bool {
        !matches!(self, Self::MaxDrawdown)
    }

    /// Compare two metric values. Returns true if `a` is better than `b`.
    ///
    /// For all metrics including MaxDrawdown, `a > b` is the correct comparison:
    /// higher Sharpe/CAGR/etc. is better, and for MaxDrawdown, -0.05 > -0.20
    /// means less negative (smaller drawdown) is better.
    pub fn is_better(&self, a: f64, b: f64) -> bool {
        a > b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metrics() -> PerformanceMetrics {
        PerformanceMetrics {
            total_return: 0.15,
            cagr: 0.12,
            sharpe: 1.5,
            sortino: 2.0,
            calmar: 1.2,
            max_drawdown: -0.10,
            win_rate: 0.55,
            profit_factor: 1.8,
            trade_count: 20,
            turnover: 3.5,
            max_consecutive_wins: 5,
            max_consecutive_losses: 3,
            avg_losing_streak: 1.5,
        }
    }

    #[test]
    fn extract_sharpe() {
        let m = sample_metrics();
        assert!((FitnessMetric::Sharpe.extract(&m) - 1.5).abs() < 1e-10);
    }

    #[test]
    fn extract_max_drawdown() {
        let m = sample_metrics();
        assert!((FitnessMetric::MaxDrawdown.extract(&m) - (-0.10)).abs() < 1e-10);
    }

    #[test]
    fn higher_better_sharpe() {
        assert!(FitnessMetric::Sharpe.is_higher_better());
    }

    #[test]
    fn higher_not_better_max_drawdown() {
        assert!(!FitnessMetric::MaxDrawdown.is_higher_better());
    }

    #[test]
    fn default_is_sharpe() {
        assert_eq!(FitnessMetric::default(), FitnessMetric::Sharpe);
    }

    #[test]
    fn is_better_sharpe() {
        assert!(FitnessMetric::Sharpe.is_better(2.0, 1.5));
        assert!(!FitnessMetric::Sharpe.is_better(1.0, 1.5));
    }

    #[test]
    fn is_better_max_drawdown() {
        // -0.05 is better than -0.20 (less negative)
        assert!(FitnessMetric::MaxDrawdown.is_better(-0.05, -0.20));
        assert!(!FitnessMetric::MaxDrawdown.is_better(-0.20, -0.05));
    }
}
