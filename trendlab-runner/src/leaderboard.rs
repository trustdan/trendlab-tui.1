//! Strategy leaderboard with configurable ranking metrics.

use crate::result::BacktestResult;
use serde::{Deserialize, Serialize};

/// Fitness metric for ranking strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitnessMetric {
    /// Sharpe ratio (default)
    Sharpe,

    /// Sortino ratio
    Sortino,

    /// Calmar ratio (return / max drawdown)
    Calmar,

    /// Total return
    TotalReturn,

    /// Annual return
    AnnualReturn,

    /// Win rate
    WinRate,

    /// Profit factor
    ProfitFactor,

    /// Custom composite score
    Composite,
}

impl FitnessMetric {
    /// Extracts the metric value from a result.
    pub fn extract(&self, result: &BacktestResult) -> f64 {
        match self {
            Self::Sharpe => result.stats.sharpe,
            Self::Sortino => result.stats.sortino,
            Self::Calmar => result.stats.calmar,
            Self::TotalReturn => result.stats.total_return,
            Self::AnnualReturn => result.stats.annual_return,
            Self::WinRate => result.stats.win_rate,
            Self::ProfitFactor => result.stats.profit_factor,
            Self::Composite => self.composite_score(result),
        }
    }

    /// Computes a composite score from multiple metrics.
    ///
    /// Composite = (Sharpe + Sortino + Calmar) / 3
    fn composite_score(&self, result: &BacktestResult) -> f64 {
        (result.stats.sharpe + result.stats.sortino + result.stats.calmar) / 3.0
    }
}

/// Strategy leaderboard for ranking backtest results.
pub struct Leaderboard {
    results: Vec<BacktestResult>,
    metric: FitnessMetric,
}

impl Leaderboard {
    /// Creates a new leaderboard with the given results.
    pub fn new(results: Vec<BacktestResult>) -> Self {
        Self {
            results,
            metric: FitnessMetric::Sharpe,
        }
    }

    /// Sets the fitness metric for ranking.
    pub fn with_metric(mut self, metric: FitnessMetric) -> Self {
        self.metric = metric;
        self
    }

    /// Returns all results sorted by the current fitness metric (descending).
    pub fn sorted(&self) -> Vec<&BacktestResult> {
        let mut sorted: Vec<_> = self.results.iter().collect();
        sorted.sort_by(|a, b| {
            let score_a = self.metric.extract(a);
            let score_b = self.metric.extract(b);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted
    }

    /// Returns the top N results by fitness.
    pub fn top_n(&self, n: usize) -> Vec<&BacktestResult> {
        self.sorted().into_iter().take(n).collect()
    }

    /// Returns the best result by fitness.
    pub fn best(&self) -> Option<&BacktestResult> {
        self.sorted().into_iter().next()
    }

    /// Returns the worst result by fitness.
    pub fn worst(&self) -> Option<&BacktestResult> {
        self.sorted().into_iter().last()
    }

    /// Filters results by minimum fitness threshold.
    pub fn filter_by_min_fitness(&self, min_fitness: f64) -> Vec<&BacktestResult> {
        self.results
            .iter()
            .filter(|r| self.metric.extract(r) >= min_fitness)
            .collect()
    }

    /// Returns summary statistics across all results.
    pub fn summary(&self) -> LeaderboardSummary {
        let scores: Vec<f64> = self.results.iter().map(|r| self.metric.extract(r)).collect();

        let mean = if !scores.is_empty() {
            scores.iter().sum::<f64>() / scores.len() as f64
        } else {
            0.0
        };

        let median = if !scores.is_empty() {
            let mut sorted_scores = scores.clone();
            sorted_scores.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mid = sorted_scores.len() / 2;
            if sorted_scores.len().is_multiple_of(2) {
                (sorted_scores[mid - 1] + sorted_scores[mid]) / 2.0
            } else {
                sorted_scores[mid]
            }
        } else {
            0.0
        };

        let min = scores
            .iter()
            .copied()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let max = scores
            .iter()
            .copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let std_dev = if scores.len() > 1 {
            let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>()
                / (scores.len() - 1) as f64;
            variance.sqrt()
        } else {
            0.0
        };

        LeaderboardSummary {
            metric: self.metric,
            count: self.results.len(),
            mean,
            median,
            std_dev,
            min,
            max,
        }
    }

    /// Returns the number of results.
    pub fn len(&self) -> usize {
        self.results.len()
    }

    /// Returns true if there are no results.
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// Returns the current fitness metric.
    pub fn metric(&self) -> FitnessMetric {
        self.metric
    }
}

/// Summary statistics for a leaderboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardSummary {
    pub metric: FitnessMetric,
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
}

impl LeaderboardSummary {
    /// Formats the summary as a human-readable string.
    pub fn format(&self) -> String {
        format!(
            "Metric: {:?}\n\
             Count: {}\n\
             Mean: {:.4}\n\
             Median: {:.4}\n\
             Std Dev: {:.4}\n\
             Min: {:.4}\n\
             Max: {:.4}",
            self.metric, self.count, self.mean, self.median, self.std_dev, self.min, self.max
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{EquityPoint, PerformanceStats, ResultMetadata};
    use chrono::{NaiveDate, Utc};
    use std::collections::HashMap;

    fn make_test_result(sharpe: f64, total_return: f64) -> BacktestResult {
        let mut stats = PerformanceStats::default();
        stats.sharpe = sharpe;
        stats.total_return = total_return;

        BacktestResult {
            run_id: format!("test_{}", sharpe),
            equity_curve: vec![EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
                equity: 100_000.0,
            }],
            trades: vec![],
            stats,
            metadata: ResultMetadata {
                timestamp: Utc::now(),
                duration_secs: 1.0,
                custom: HashMap::new(),
                config: None,
            },
        }
    }

    #[test]
    fn test_leaderboard_sorting() {
        let results = vec![
            make_test_result(1.5, 0.20),
            make_test_result(2.5, 0.30),
            make_test_result(0.5, 0.10),
        ];

        let leaderboard = Leaderboard::new(results).with_metric(FitnessMetric::Sharpe);
        let sorted = leaderboard.sorted();

        // Should be sorted by Sharpe descending: 2.5, 1.5, 0.5
        assert_eq!(sorted[0].stats.sharpe, 2.5);
        assert_eq!(sorted[1].stats.sharpe, 1.5);
        assert_eq!(sorted[2].stats.sharpe, 0.5);
    }

    #[test]
    fn test_leaderboard_top_n() {
        let results = vec![
            make_test_result(1.5, 0.20),
            make_test_result(2.5, 0.30),
            make_test_result(0.5, 0.10),
            make_test_result(3.0, 0.35),
        ];

        let leaderboard = Leaderboard::new(results);
        let top_2 = leaderboard.top_n(2);

        assert_eq!(top_2.len(), 2);
        assert_eq!(top_2[0].stats.sharpe, 3.0);
        assert_eq!(top_2[1].stats.sharpe, 2.5);
    }

    #[test]
    fn test_leaderboard_best_worst() {
        let results = vec![
            make_test_result(1.5, 0.20),
            make_test_result(2.5, 0.30),
            make_test_result(0.5, 0.10),
        ];

        let leaderboard = Leaderboard::new(results);

        let best = leaderboard.best().unwrap();
        assert_eq!(best.stats.sharpe, 2.5);

        let worst = leaderboard.worst().unwrap();
        assert_eq!(worst.stats.sharpe, 0.5);
    }

    #[test]
    fn test_leaderboard_filter_by_min_fitness() {
        let results = vec![
            make_test_result(1.5, 0.20),
            make_test_result(2.5, 0.30),
            make_test_result(0.5, 0.10),
        ];

        let leaderboard = Leaderboard::new(results);
        let filtered = leaderboard.filter_by_min_fitness(1.0);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.stats.sharpe >= 1.0));
    }

    #[test]
    fn test_leaderboard_summary() {
        let results = vec![
            make_test_result(1.0, 0.20),
            make_test_result(2.0, 0.30),
            make_test_result(3.0, 0.40),
        ];

        let leaderboard = Leaderboard::new(results);
        let summary = leaderboard.summary();

        assert_eq!(summary.count, 3);
        assert_eq!(summary.mean, 2.0);
        assert_eq!(summary.median, 2.0);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 3.0);
    }

    #[test]
    fn test_fitness_metric_extraction() {
        let result = make_test_result(1.5, 0.25);

        assert_eq!(FitnessMetric::Sharpe.extract(&result), 1.5);
        assert_eq!(FitnessMetric::TotalReturn.extract(&result), 0.25);
    }

    #[test]
    fn test_different_metrics() {
        let mut result1 = make_test_result(1.0, 0.10);
        result1.stats.sortino = 2.0;

        let mut result2 = make_test_result(2.0, 0.20);
        result2.stats.sortino = 1.0;

        let results = vec![result1, result2];

        // By Sharpe: result2 wins
        let leaderboard_sharpe = Leaderboard::new(results.clone()).with_metric(FitnessMetric::Sharpe);
        assert_eq!(leaderboard_sharpe.best().unwrap().stats.sharpe, 2.0);

        // By Sortino: result1 wins
        let leaderboard_sortino = Leaderboard::new(results).with_metric(FitnessMetric::Sortino);
        assert_eq!(leaderboard_sortino.best().unwrap().stats.sortino, 2.0);
    }
}
