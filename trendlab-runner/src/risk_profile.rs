//! Risk profiles and composite ranking — weighted metric aggregation with
//! rank-based normalization.
//!
//! Four named profiles weight cross-symbol metrics differently:
//! - **Balanced**: equal weight (default exploration)
//! - **Conservative**: emphasizes tail risk, drawdown, consistency
//! - **Aggressive**: emphasizes returns, Sharpe, hit rate
//! - **TrendOptions**: emphasizes hit rate, consecutive losses, OOS Sharpe
//!
//! Rank normalization: before applying weights, raw metric values are replaced
//! with their percentile rank (0.0 = worst, 1.0 = best) within the current
//! population. This ensures metrics with different units contribute proportionally.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use trendlab_core::domain::FullHash;

use crate::cross_leaderboard::CrossSymbolEntry;

/// Risk profile for composite ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RiskProfile {
    #[default]
    Balanced,
    Conservative,
    Aggressive,
    TrendOptions,
}

/// Which metric to sort the cross-symbol leaderboard by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RankingMetric {
    #[default]
    AvgSharpe,
    MinSharpe,
    GeoMeanCagr,
    HitRate,
    /// Stub for Phase 11 walk-forward integration.
    MeanOosSharpe,
    /// Composite score using the active risk profile.
    Composite,
}

/// Internal: weights for each metric dimension.
struct ProfileWeights {
    avg_sharpe: f64,
    geo_mean_cagr: f64,
    hit_rate: f64,
    worst_drawdown: f64,
    avg_trade_count: f64,
    tail_risk: f64,
    consistency: f64,
}

impl RiskProfile {
    fn weights(&self) -> ProfileWeights {
        match self {
            Self::Balanced => ProfileWeights {
                avg_sharpe: 1.0,
                geo_mean_cagr: 1.0,
                hit_rate: 1.0,
                worst_drawdown: 1.0,
                avg_trade_count: 0.5,
                tail_risk: 1.0,
                consistency: 1.0,
            },
            Self::Conservative => ProfileWeights {
                avg_sharpe: 1.0,
                geo_mean_cagr: 0.5,
                hit_rate: 1.0,
                worst_drawdown: 2.0,
                avg_trade_count: 0.5,
                tail_risk: 2.0,
                consistency: 2.0,
            },
            Self::Aggressive => ProfileWeights {
                avg_sharpe: 2.0,
                geo_mean_cagr: 2.0,
                hit_rate: 1.5,
                worst_drawdown: 0.5,
                avg_trade_count: 0.5,
                tail_risk: 0.5,
                consistency: 0.5,
            },
            Self::TrendOptions => ProfileWeights {
                avg_sharpe: 1.0,
                geo_mean_cagr: 0.5,
                hit_rate: 2.0,
                worst_drawdown: 1.0,
                avg_trade_count: 0.5,
                tail_risk: 1.0,
                consistency: 1.5,
            },
        }
    }
}

/// Compute composite scores for a set of cross-symbol entries under a risk profile.
///
/// Returns a map of `FullHash → composite_score`. Higher is better.
///
/// Each metric dimension is rank-normalized to [0.0, 1.0] within the population,
/// then weighted by the profile. The composite score is the weighted sum.
pub fn compute_composite_scores(
    entries: &[&CrossSymbolEntry],
    profile: RiskProfile,
) -> HashMap<FullHash, f64> {
    if entries.is_empty() {
        return HashMap::new();
    }

    let weights = profile.weights();

    // Extract raw metric vectors (one per dimension)
    let avg_sharpes: Vec<f64> = entries.iter().map(|e| e.avg_sharpe).collect();
    let geo_cagrs: Vec<f64> = entries.iter().map(|e| e.geo_mean_cagr).collect();
    let hit_rates: Vec<f64> = entries.iter().map(|e| e.hit_rate).collect();
    // For drawdown, less negative is better → higher is better
    let worst_dds: Vec<f64> = entries.iter().map(|e| e.worst_max_drawdown).collect();
    let avg_trades: Vec<f64> = entries.iter().map(|e| e.avg_trade_count).collect();
    let min_sharpes: Vec<f64> = entries.iter().map(|e| e.min_sharpe).collect();

    // Tail risk: use CVaR if available, else 0.0
    // For CVaR, less negative is better → higher is better
    let cvar_values: Vec<f64> = entries
        .iter()
        .map(|e| {
            e.tail_metrics
                .as_ref()
                .and_then(|t| t.cvar_95)
                .unwrap_or(0.0)
        })
        .collect();

    // Rank-normalize each dimension
    let r_sharpe = rank_normalize(&avg_sharpes, true);
    let r_cagr = rank_normalize(&geo_cagrs, true);
    let r_hit = rank_normalize(&hit_rates, true);
    let r_dd = rank_normalize(&worst_dds, true); // higher (less negative) is better
    let r_trades = rank_normalize(&avg_trades, true); // more trades = more statistical significance
    let r_cvar = rank_normalize(&cvar_values, true); // higher (less negative) is better
    let r_consistency = rank_normalize(&min_sharpes, true);

    let mut scores = HashMap::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        let score = weights.avg_sharpe * r_sharpe[i]
            + weights.geo_mean_cagr * r_cagr[i]
            + weights.hit_rate * r_hit[i]
            + weights.worst_drawdown * r_dd[i]
            + weights.avg_trade_count * r_trades[i]
            + weights.tail_risk * r_cvar[i]
            + weights.consistency * r_consistency[i];
        scores.insert(entry.full_hash.clone(), score);
    }

    scores
}

/// Rank-normalize a vector of values to [0.0, 1.0].
///
/// Each value is replaced with its percentile rank within the population.
/// Tied values receive the average rank. If `higher_is_better` is false,
/// the ranks are inverted (1.0 - rank).
///
/// Single-element vectors return [0.5].
pub fn rank_normalize(values: &[f64], higher_is_better: bool) -> Vec<f64> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![0.5];
    }

    // Create index-value pairs, sorted by value
    let mut indexed: Vec<(usize, f64)> = values.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Assign ranks (1-based), handling ties with average rank
    let mut ranks = vec![0.0_f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        // Find the end of the tied group
        while j < n && (indexed[j].1 - indexed[i].1).abs() < 1e-15 {
            j += 1;
        }
        // Average rank for the tied group (1-based)
        let avg_rank = (i + 1 + j) as f64 / 2.0;
        for idx in &indexed[i..j] {
            ranks[idx.0] = avg_rank;
        }
        i = j;
    }

    // Normalize to [0.0, 1.0]
    let max_rank = n as f64;
    let mut normalized: Vec<f64> = ranks.iter().map(|r| (r - 1.0) / (max_rank - 1.0)).collect();

    if !higher_is_better {
        for v in &mut normalized {
            *v = 1.0 - *v;
        }
    }

    normalized
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use trendlab_core::fingerprint::ComponentConfig;

    fn make_config(signal_type: &str, lookback: f64) -> trendlab_core::fingerprint::StrategyConfig {
        trendlab_core::fingerprint::StrategyConfig {
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

    fn make_entry(
        signal: &str,
        lookback: f64,
        sharpe: f64,
        cagr: f64,
        hit_rate: f64,
        worst_dd: f64,
    ) -> CrossSymbolEntry {
        let config = make_config(signal, lookback);
        let ts = chrono::NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();

        CrossSymbolEntry {
            config_hash: config.config_hash(),
            full_hash: config.full_hash(),
            config,
            avg_sharpe: sharpe,
            min_sharpe: sharpe * 0.8,
            max_sharpe: sharpe * 1.2,
            geo_mean_cagr: cagr,
            hit_rate,
            worst_max_drawdown: worst_dd,
            avg_trade_count: 20.0,
            tail_metrics: None,
            symbol_count: 3,
            symbol_metrics: HashMap::new(),
            symbol_equity_curves: HashMap::new(),
            avg_stickiness: None,
            symbol_stickiness: HashMap::new(),
            robustness: None,
            has_catastrophic: false,
            session_id: "test".into(),
            timestamp: ts,
            iteration: 0,
        }
    }

    // ── Rank normalization ──

    #[test]
    fn rank_normalize_basic() {
        let values = vec![10.0, 30.0, 20.0, 40.0, 50.0];
        let ranks = rank_normalize(&values, true);
        // 10→0.0, 20→0.25, 30→0.5, 40→0.75, 50→1.0
        assert!((ranks[0] - 0.0).abs() < 1e-10); // 10
        assert!((ranks[1] - 0.5).abs() < 1e-10); // 30
        assert!((ranks[2] - 0.25).abs() < 1e-10); // 20
        assert!((ranks[3] - 0.75).abs() < 1e-10); // 40
        assert!((ranks[4] - 1.0).abs() < 1e-10); // 50
    }

    #[test]
    fn rank_normalize_ties() {
        let values = vec![1.0, 2.0, 2.0, 3.0];
        let ranks = rank_normalize(&values, true);
        // 1→0.0, 2→avg(0.333, 0.667)=0.5, 3→1.0
        assert!((ranks[0] - 0.0).abs() < 1e-10);
        assert!((ranks[1] - ranks[2]).abs() < 1e-10); // tied
        assert!((ranks[1] - 0.5).abs() < 1e-10);
        assert!((ranks[3] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn rank_normalize_single() {
        let ranks = rank_normalize(&[42.0], true);
        assert_eq!(ranks, vec![0.5]);
    }

    #[test]
    fn rank_normalize_empty() {
        let ranks = rank_normalize(&[], true);
        assert!(ranks.is_empty());
    }

    #[test]
    fn rank_normalize_inverted() {
        let values = vec![10.0, 20.0, 30.0];
        let ranks = rank_normalize(&values, false);
        // 10→1.0, 20→0.5, 30→0.0 (inverted)
        assert!((ranks[0] - 1.0).abs() < 1e-10);
        assert!((ranks[1] - 0.5).abs() < 1e-10);
        assert!((ranks[2] - 0.0).abs() < 1e-10);
    }

    // ── Rescaling invariance ──

    #[test]
    fn composite_invariant_under_rescaling() {
        let e1 = make_entry("donchian", 50.0, 1.0, 0.10, 0.6, -0.10);
        let e2 = make_entry("donchian", 100.0, 2.0, 0.15, 0.8, -0.08);
        let e3 = make_entry("bollinger", 20.0, 1.5, 0.12, 0.7, -0.12);

        let refs: Vec<&CrossSymbolEntry> = vec![&e1, &e2, &e3];
        let scores = compute_composite_scores(&refs, RiskProfile::Balanced);

        // Now rescale all avg_sharpe by 10x
        let mut e1r = e1.clone();
        let mut e2r = e2.clone();
        let mut e3r = e3.clone();
        e1r.avg_sharpe *= 10.0;
        e2r.avg_sharpe *= 10.0;
        e3r.avg_sharpe *= 10.0;

        let refs_r: Vec<&CrossSymbolEntry> = vec![&e1r, &e2r, &e3r];
        let scores_r = compute_composite_scores(&refs_r, RiskProfile::Balanced);

        // Ordering should be identical
        let mut order: Vec<_> = scores.iter().collect();
        order.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        let mut order_r: Vec<_> = scores_r.iter().collect();
        order_r.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

        for (a, b) in order.iter().zip(order_r.iter()) {
            assert_eq!(a.0, b.0, "Ranking order changed after rescaling");
        }
    }

    // ── Different profiles produce different rankings ──

    #[test]
    fn profiles_produce_different_top_entry() {
        // 3 entries where Aggressive and Conservative disagree on #1:
        // e1: best sharpe+cagr, worst drawdown, worst consistency (min_sharpe)
        // e2: middle everything
        // e3: worst sharpe+cagr, best drawdown, best consistency
        let mut e1 = make_entry("donchian", 50.0, 3.0, 0.25, 0.9, -0.40);
        e1.min_sharpe = -0.5; // Worst consistency: high avg but terrible min

        let mut e2 = make_entry("bollinger", 20.0, 1.5, 0.10, 0.7, -0.15);
        e2.min_sharpe = 0.8; // Middle consistency

        let mut e3 = make_entry("keltner", 30.0, 0.5, 0.03, 0.5, -0.03);
        e3.min_sharpe = 0.4; // Best consistency (also best drawdown)

        let refs: Vec<&CrossSymbolEntry> = vec![&e1, &e2, &e3];

        let aggressive = compute_composite_scores(&refs, RiskProfile::Aggressive);
        let conservative = compute_composite_scores(&refs, RiskProfile::Conservative);

        // Aggressive's top entry should be e1 (highest sharpe/cagr)
        let mut agg_ranked: Vec<_> = aggressive.iter().collect();
        agg_ranked.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        assert_eq!(
            agg_ranked[0].0, &e1.full_hash,
            "Aggressive top should be e1 (high returns)"
        );

        // Conservative's top entry should NOT be e1 (worst drawdown + worst consistency)
        let mut con_ranked: Vec<_> = conservative.iter().collect();
        con_ranked.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
        assert_ne!(
            con_ranked[0].0, &e1.full_hash,
            "Conservative top should not be e1 (worst drawdown + consistency)"
        );
    }

    // ── Default values ──

    #[test]
    fn default_profile_is_balanced() {
        assert_eq!(RiskProfile::default(), RiskProfile::Balanced);
    }

    #[test]
    fn default_metric_is_avg_sharpe() {
        assert_eq!(RankingMetric::default(), RankingMetric::AvgSharpe);
    }

    // ── Serialization ──

    #[test]
    fn risk_profile_serialization() {
        let p = RiskProfile::Conservative;
        let json = serde_json::to_string(&p).unwrap();
        let deser: RiskProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(p, deser);
    }

    #[test]
    fn ranking_metric_serialization() {
        let m = RankingMetric::HitRate;
        let json = serde_json::to_string(&m).unwrap();
        let deser: RankingMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(m, deser);
    }

    #[test]
    fn composite_empty_entries() {
        let scores = compute_composite_scores(&[], RiskProfile::Balanced);
        assert!(scores.is_empty());
    }

    #[test]
    fn composite_single_entry() {
        let e = make_entry("donchian", 50.0, 1.5, 0.10, 0.7, -0.08);
        let refs: Vec<&CrossSymbolEntry> = vec![&e];
        let scores = compute_composite_scores(&refs, RiskProfile::Balanced);

        // Single entry: all rank-normalized values should be 0.5
        // Composite = sum of (weight * 0.5) for each dimension
        let expected = (1.0 + 1.0 + 1.0 + 1.0 + 0.5 + 1.0 + 1.0) * 0.5;
        let score = scores[&e.full_hash];
        assert!(
            (score - expected).abs() < 1e-10,
            "Expected {expected}, got {score}"
        );
    }
}
