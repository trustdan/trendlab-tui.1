//! Per-symbol leaderboard — bounded, deduplicated, sorted by fitness.
//!
//! Each symbol maintains its own leaderboard of the top N strategy configurations.
//! Deduplication key: `full_hash` (exact config + params). If a config with the
//! same full_hash arrives with a better fitness score, it replaces the existing entry.
//! If worse, it is skipped.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::fitness::FitnessMetric;
use crate::runner::BacktestResult;
use trendlab_core::domain::FullHash;

/// A single entry in the leaderboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub result: BacktestResult,
    pub fitness_score: f64,
    pub iteration: usize,
    pub session_id: String,
    pub timestamp: NaiveDateTime,
}

/// Outcome of an insert operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertResult {
    /// New entry added to the leaderboard.
    Inserted,
    /// Replaced an existing entry with the same full_hash (better score).
    Replaced,
    /// Skipped: duplicate with worse or equal score, or NaN fitness.
    Skipped,
}

/// Per-symbol leaderboard: top N strategies ranked by fitness.
#[derive(Debug)]
pub struct SymbolLeaderboard {
    symbol: String,
    entries: Vec<LeaderboardEntry>,
    max_size: usize,
    fitness_metric: FitnessMetric,
}

impl SymbolLeaderboard {
    pub fn new(symbol: String, max_size: usize, fitness_metric: FitnessMetric) -> Self {
        Self {
            symbol,
            entries: Vec::with_capacity(max_size.min(1024)),
            max_size,
            fitness_metric,
        }
    }

    /// Insert an entry. Returns the outcome.
    ///
    /// - Rejects entries with non-finite fitness scores.
    /// - Deduplicates by `full_hash`: replaces if better, skips if worse.
    /// - After insert, trims to `max_size` by removing the worst entry.
    pub fn insert(&mut self, entry: LeaderboardEntry) -> InsertResult {
        // Reject NaN/Inf fitness
        if !entry.fitness_score.is_finite() {
            return InsertResult::Skipped;
        }

        let entry_hash = entry.result.config.full_hash();

        // Check for duplicate
        if let Some(idx) = self.find_by_hash(&entry_hash) {
            if self
                .fitness_metric
                .is_better(entry.fitness_score, self.entries[idx].fitness_score)
            {
                self.entries[idx] = entry;
                self.sort_entries();
                return InsertResult::Replaced;
            }
            return InsertResult::Skipped;
        }

        // New entry: insert if there's room or it beats the worst
        if self.entries.len() < self.max_size {
            self.entries.push(entry);
            self.sort_entries();
            InsertResult::Inserted
        } else if let Some(worst) = self.entries.last() {
            if self
                .fitness_metric
                .is_better(entry.fitness_score, worst.fitness_score)
            {
                self.entries.pop();
                self.entries.push(entry);
                self.sort_entries();
                InsertResult::Inserted
            } else {
                InsertResult::Skipped
            }
        } else {
            // Empty entries with max_size 0
            InsertResult::Skipped
        }
    }

    pub fn entries(&self) -> &[LeaderboardEntry] {
        &self.entries
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn fitness_metric(&self) -> FitnessMetric {
        self.fitness_metric
    }

    fn find_by_hash(&self, hash: &FullHash) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.result.config.full_hash() == *hash)
    }

    fn sort_entries(&mut self) {
        // Sort descending by fitness score (best first).
        // FitnessMetric.is_better(a, b) = a > b for all metrics,
        // so descending f64 order is correct.
        self.entries.sort_by(|a, b| {
            b.fitness_score
                .partial_cmp(&a.fitness_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::PerformanceMetrics;
    use std::collections::{BTreeMap, HashMap};
    use trendlab_core::fingerprint::{ComponentConfig, StrategyConfig};

    fn make_config(signal_type: &str, lookback: f64) -> StrategyConfig {
        StrategyConfig {
            signal: ComponentConfig {
                component_type: signal_type.into(),
                params: {
                    let mut m = BTreeMap::new();
                    m.insert("lookback".into(), lookback);
                    m
                },
            },
            position_manager: ComponentConfig {
                component_type: "atr_trailing".into(),
                params: BTreeMap::new(),
            },
            execution_model: ComponentConfig {
                component_type: "next_bar_open".into(),
                params: BTreeMap::new(),
            },
            signal_filter: ComponentConfig {
                component_type: "no_filter".into(),
                params: BTreeMap::new(),
            },
        }
    }

    fn make_metrics(sharpe: f64) -> PerformanceMetrics {
        PerformanceMetrics {
            total_return: 0.1,
            cagr: 0.08,
            sharpe,
            sortino: 1.0,
            calmar: 0.5,
            max_drawdown: -0.1,
            win_rate: 0.5,
            profit_factor: 1.5,
            trade_count: 10,
            turnover: 2.0,
            max_consecutive_wins: 3,
            max_consecutive_losses: 2,
            avg_losing_streak: 1.5,
        }
    }

    fn make_entry(
        signal_type: &str,
        lookback: f64,
        sharpe: f64,
        iteration: usize,
    ) -> LeaderboardEntry {
        let config = make_config(signal_type, lookback);
        let metrics = make_metrics(sharpe);
        LeaderboardEntry {
            result: BacktestResult {
                metrics: metrics.clone(),
                trades: vec![],
                equity_curve: vec![100_000.0],
                config,
                symbol: "SPY".into(),
                start_date: "2024-01-02".into(),
                end_date: "2024-12-31".into(),
                initial_capital: 100_000.0,
                dataset_hash: "test".into(),
                has_synthetic: false,
                signal_count: 5,
                bar_count: 252,
                warmup_bars: 50,
                void_bar_rates: HashMap::new(),
                data_quality_warnings: vec![],
                stickiness: None,
            },
            fitness_score: sharpe,
            iteration,
            session_id: "test-session".into(),
            timestamp: NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        }
    }

    #[test]
    fn insert_unique_entries() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        let r1 = lb.insert(make_entry("donchian", 50.0, 1.5, 0));
        let r2 = lb.insert(make_entry("donchian", 100.0, 2.0, 1));
        let r3 = lb.insert(make_entry("bollinger", 20.0, 1.0, 2));

        assert_eq!(r1, InsertResult::Inserted);
        assert_eq!(r2, InsertResult::Inserted);
        assert_eq!(r3, InsertResult::Inserted);
        assert_eq!(lb.len(), 3);
    }

    #[test]
    fn entries_sorted_best_first() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        lb.insert(make_entry("donchian", 50.0, 1.0, 0));
        lb.insert(make_entry("donchian", 100.0, 3.0, 1));
        lb.insert(make_entry("bollinger", 20.0, 2.0, 2));

        let scores: Vec<f64> = lb.entries().iter().map(|e| e.fitness_score).collect();
        assert_eq!(scores, vec![3.0, 2.0, 1.0]);
    }

    #[test]
    fn dedup_replaces_on_better_score() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        lb.insert(make_entry("donchian", 50.0, 1.0, 0));
        assert_eq!(lb.len(), 1);

        // Same config (same full_hash), better score
        let r = lb.insert(make_entry("donchian", 50.0, 2.0, 1));
        assert_eq!(r, InsertResult::Replaced);
        assert_eq!(lb.len(), 1);
        assert_eq!(lb.entries()[0].fitness_score, 2.0);
        assert_eq!(lb.entries()[0].iteration, 1);
    }

    #[test]
    fn dedup_skips_on_worse_score() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        lb.insert(make_entry("donchian", 50.0, 2.0, 0));

        // Same config, worse score
        let r = lb.insert(make_entry("donchian", 50.0, 1.0, 1));
        assert_eq!(r, InsertResult::Skipped);
        assert_eq!(lb.len(), 1);
        assert_eq!(lb.entries()[0].fitness_score, 2.0);
        assert_eq!(lb.entries()[0].iteration, 0);
    }

    #[test]
    fn trims_to_max_size() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 3, FitnessMetric::Sharpe);

        lb.insert(make_entry("donchian", 50.0, 1.0, 0));
        lb.insert(make_entry("donchian", 100.0, 2.0, 1));
        lb.insert(make_entry("bollinger", 20.0, 3.0, 2));
        assert_eq!(lb.len(), 3);

        // Insert a 4th entry that beats the worst
        let r = lb.insert(make_entry("keltner", 20.0, 4.0, 3));
        assert_eq!(r, InsertResult::Inserted);
        assert_eq!(lb.len(), 3);

        // Worst (1.0) should have been evicted
        let scores: Vec<f64> = lb.entries().iter().map(|e| e.fitness_score).collect();
        assert_eq!(scores, vec![4.0, 3.0, 2.0]);
    }

    #[test]
    fn rejects_when_worse_than_all_and_full() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 2, FitnessMetric::Sharpe);

        lb.insert(make_entry("donchian", 50.0, 3.0, 0));
        lb.insert(make_entry("donchian", 100.0, 2.0, 1));

        // Worse than both
        let r = lb.insert(make_entry("bollinger", 20.0, 1.0, 2));
        assert_eq!(r, InsertResult::Skipped);
        assert_eq!(lb.len(), 2);
    }

    #[test]
    fn rejects_nan_fitness() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        let r = lb.insert(make_entry("donchian", 50.0, f64::NAN, 0));
        assert_eq!(r, InsertResult::Skipped);
        assert_eq!(lb.len(), 0);
    }

    #[test]
    fn rejects_inf_fitness() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        let r = lb.insert(make_entry("donchian", 50.0, f64::INFINITY, 0));
        assert_eq!(r, InsertResult::Skipped);
        assert_eq!(lb.len(), 0);
    }

    #[test]
    fn different_params_are_not_duplicates() {
        let mut lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);

        lb.insert(make_entry("donchian", 50.0, 1.0, 0));
        lb.insert(make_entry("donchian", 100.0, 2.0, 1));

        // Same signal type but different lookback → different full_hash → both kept
        assert_eq!(lb.len(), 2);
    }

    #[test]
    fn empty_leaderboard_accessors() {
        let lb = SymbolLeaderboard::new("SPY".into(), 10, FitnessMetric::Sharpe);
        assert!(lb.is_empty());
        assert_eq!(lb.len(), 0);
        assert_eq!(lb.symbol(), "SPY");
        assert_eq!(lb.entries().len(), 0);
    }
}
