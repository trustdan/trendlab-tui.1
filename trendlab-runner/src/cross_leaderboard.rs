//! Cross-symbol leaderboard — aggregates per-symbol results by strategy config.
//!
//! For each unique strategy configuration (keyed by `full_hash`), the cross-symbol
//! leaderboard tracks aggregated metrics across all symbols that have been tested
//! with that configuration. This enables apples-to-apples comparison of strategy
//! structures across the entire universe.
//!
//! Designed for incremental update: as new per-symbol results arrive during a YOLO
//! session, the aggregates are recomputed efficiently.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use trendlab_core::domain::{ConfigHash, FullHash};
use trendlab_core::fingerprint::StrategyConfig;

use trendlab_core::engine::stickiness::StickinessMetrics;

use crate::metrics::PerformanceMetrics;
use crate::promotion::RobustnessResult;
use crate::risk_profile::RankingMetric;
use crate::tail_metrics::{compute_tail_metrics, TailMetrics};

/// Aggregated stickiness metrics across multiple symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedStickiness {
    pub avg_median_holding_bars: f64,
    pub worst_median_holding_bars: f64,
    pub avg_exit_trigger_rate: f64,
    pub worst_exit_trigger_rate: f64,
    pub avg_reference_chase_ratio: f64,
    pub worst_reference_chase_ratio: f64,
    pub symbol_count: usize,
    /// True if any symbol shows pathological stickiness.
    pub is_pathological: bool,
}

/// Check if stickiness metrics indicate a pathological configuration.
///
/// Pathological = median holding > 100 bars OR exit trigger rate < 0.05.
pub fn is_pathological_stickiness(s: &StickinessMetrics) -> bool {
    s.median_holding_bars > 100.0 || s.exit_trigger_rate < 0.05
}

/// A single entry in the cross-symbol leaderboard.
///
/// Aggregates results from the same strategy config (`full_hash`) across
/// multiple symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSymbolEntry {
    pub config: StrategyConfig,
    pub config_hash: ConfigHash,
    pub full_hash: FullHash,

    // ── Aggregated metrics ──
    pub avg_sharpe: f64,
    pub min_sharpe: f64,
    pub max_sharpe: f64,
    pub geo_mean_cagr: f64,
    pub hit_rate: f64,
    pub worst_max_drawdown: f64,
    pub avg_trade_count: f64,

    // ── Tail risk ──
    pub tail_metrics: Option<TailMetrics>,

    // ── Per-symbol breakdown ──
    pub symbol_count: usize,
    pub symbol_metrics: HashMap<String, PerformanceMetrics>,
    /// Per-symbol equity curves for tail metric recomputation.
    /// Per-symbol equity curves for tail metric recomputation.
    #[serde(skip)]
    pub(crate) symbol_equity_curves: HashMap<String, Vec<f64>>,

    // ── Stickiness ──
    #[serde(default)]
    pub avg_stickiness: Option<AggregatedStickiness>,
    /// Per-symbol stickiness for aggregation.
    #[serde(skip)]
    pub(crate) symbol_stickiness: HashMap<String, StickinessMetrics>,

    // ── Robustness (promotion ladder) ──
    #[serde(default)]
    pub robustness: Option<RobustnessResult>,

    // ── Flags ──
    pub has_catastrophic: bool,

    // ── Provenance ──
    pub session_id: String,
    pub timestamp: NaiveDateTime,
    pub iteration: usize,
}

/// Cross-symbol leaderboard: top N strategy configs ranked across all symbols.
#[derive(Debug)]
pub struct CrossSymbolLeaderboard {
    entries: HashMap<FullHash, CrossSymbolEntry>,
    max_size: usize,
    catastrophic_threshold: f64,
}

impl CrossSymbolLeaderboard {
    pub fn new(max_size: usize, catastrophic_threshold: f64) -> Self {
        Self {
            entries: HashMap::with_capacity(max_size.min(1024)),
            max_size,
            catastrophic_threshold,
        }
    }

    /// Insert or update a result for a (config, symbol) pair.
    ///
    /// If the `full_hash` already exists, the new symbol's metrics are merged
    /// and aggregates are recomputed. If it's a new config, a fresh entry is created.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_result(
        &mut self,
        symbol: &str,
        metrics: PerformanceMetrics,
        equity_curve: &[f64],
        config: &StrategyConfig,
        session_id: &str,
        iteration: usize,
        timestamp: NaiveDateTime,
    ) {
        let full_hash = config.full_hash();

        let entry = self
            .entries
            .entry(full_hash.clone())
            .or_insert_with(|| CrossSymbolEntry {
                config: config.clone(),
                config_hash: config.config_hash(),
                full_hash: full_hash.clone(),
                avg_sharpe: 0.0,
                min_sharpe: f64::INFINITY,
                max_sharpe: f64::NEG_INFINITY,
                geo_mean_cagr: 0.0,
                hit_rate: 0.0,
                worst_max_drawdown: 0.0,
                avg_trade_count: 0.0,
                tail_metrics: None,
                symbol_count: 0,
                symbol_metrics: HashMap::new(),
                symbol_equity_curves: HashMap::new(),
                avg_stickiness: None,
                symbol_stickiness: HashMap::new(),
                robustness: None,
                has_catastrophic: false,
                session_id: session_id.to_string(),
                timestamp,
                iteration,
            });

        // Store per-symbol data
        entry
            .symbol_metrics
            .insert(symbol.to_string(), metrics.clone());
        entry
            .symbol_equity_curves
            .insert(symbol.to_string(), equity_curve.to_vec());
        entry.symbol_count = entry.symbol_metrics.len();

        // Update provenance to latest
        if iteration > entry.iteration {
            entry.iteration = iteration;
            entry.timestamp = timestamp;
        }

        // Recompute aggregates
        recompute_aggregates(entry, self.catastrophic_threshold);
    }

    /// Get all entries sorted by the specified ranking metric.
    ///
    /// For `RankingMetric::Composite`, the caller must provide composite scores
    /// via `get_ranked_by_scores()` instead. This method uses the raw metric value.
    pub fn get_ranked(&self, metric: RankingMetric) -> Vec<&CrossSymbolEntry> {
        let mut entries: Vec<&CrossSymbolEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| {
            let va = extract_ranking_metric(a, metric);
            let vb = extract_ranking_metric(b, metric);
            vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
        });
        entries
    }

    /// Get all entries sorted by pre-computed scores (used for composite ranking).
    pub fn get_ranked_by_scores(&self, scores: &HashMap<FullHash, f64>) -> Vec<&CrossSymbolEntry> {
        let mut entries: Vec<&CrossSymbolEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| {
            let sa = scores.get(&a.full_hash).copied().unwrap_or(0.0);
            let sb = scores.get(&b.full_hash).copied().unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        entries
    }

    /// Set per-symbol stickiness and recompute aggregated stickiness.
    pub fn set_stickiness(
        &mut self,
        full_hash: &FullHash,
        symbol: &str,
        stickiness: StickinessMetrics,
    ) {
        if let Some(entry) = self.entries.get_mut(full_hash) {
            entry
                .symbol_stickiness
                .insert(symbol.to_string(), stickiness);
            recompute_stickiness(entry);
        }
    }

    /// Set robustness result for a strategy configuration.
    pub fn set_robustness(&mut self, full_hash: &FullHash, robustness: RobustnessResult) {
        if let Some(entry) = self.entries.get_mut(full_hash) {
            entry.robustness = Some(robustness);
        }
    }

    pub fn entries(&self) -> &HashMap<FullHash, CrossSymbolEntry> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Trim to max_size by removing lowest-ranked entries.
    ///
    /// Uses avg_sharpe as the default trimming metric.
    pub fn trim(&mut self) {
        if self.entries.len() <= self.max_size {
            return;
        }

        let mut ranked: Vec<(FullHash, f64)> = self
            .entries
            .iter()
            .map(|(h, e)| (h.clone(), e.avg_sharpe))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let to_remove: Vec<FullHash> = ranked
            .into_iter()
            .skip(self.max_size)
            .map(|(h, _)| h)
            .collect();

        for hash in to_remove {
            self.entries.remove(&hash);
        }
    }
}

/// Extract a ranking metric value from a cross-symbol entry.
pub fn extract_ranking_metric(entry: &CrossSymbolEntry, metric: RankingMetric) -> f64 {
    match metric {
        RankingMetric::AvgSharpe => entry.avg_sharpe,
        RankingMetric::MinSharpe => entry.min_sharpe,
        RankingMetric::GeoMeanCagr => entry.geo_mean_cagr,
        RankingMetric::HitRate => entry.hit_rate,
        // Phase 11 stub: use avg_sharpe as placeholder for OOS Sharpe
        RankingMetric::MeanOosSharpe => entry.avg_sharpe,
        // Composite requires external scores; fall back to avg_sharpe
        RankingMetric::Composite => entry.avg_sharpe,
    }
}

/// Recompute all aggregate metrics from per-symbol data.
fn recompute_aggregates(entry: &mut CrossSymbolEntry, catastrophic_threshold: f64) {
    let metrics: Vec<&PerformanceMetrics> = entry.symbol_metrics.values().collect();
    let n = metrics.len() as f64;

    if metrics.is_empty() {
        return;
    }

    // Sharpe: avg, min, max
    let sharpes: Vec<f64> = metrics.iter().map(|m| m.sharpe).collect();
    entry.avg_sharpe = sharpes.iter().sum::<f64>() / n;
    entry.min_sharpe = sharpes.iter().copied().fold(f64::INFINITY, f64::min);
    entry.max_sharpe = sharpes.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    // Geometric mean CAGR
    entry.geo_mean_cagr = geo_mean_cagr(&metrics);

    // Hit rate: fraction with positive total return
    let profitable = metrics.iter().filter(|m| m.total_return > 0.0).count();
    entry.hit_rate = profitable as f64 / n;

    // Worst max drawdown (most negative)
    entry.worst_max_drawdown = metrics
        .iter()
        .map(|m| m.max_drawdown)
        .fold(0.0_f64, f64::min);

    // Average trade count
    entry.avg_trade_count = metrics.iter().map(|m| m.trade_count as f64).sum::<f64>() / n;

    // Catastrophic loss flag
    entry.has_catastrophic = metrics.iter().any(|m| m.cagr < catastrophic_threshold);

    // Tail metrics: compute from pooled equity curves
    recompute_tail_metrics(entry);
}

/// Compute tail metrics from pooled equity curve returns across all symbols.
fn recompute_tail_metrics(entry: &mut CrossSymbolEntry) {
    // Pool all equity curves into one long series for tail analysis
    let mut pooled_returns: Vec<f64> = Vec::new();
    for eq in entry.symbol_equity_curves.values() {
        if eq.len() >= 2 {
            for w in eq.windows(2) {
                if w[0] > 0.0 {
                    pooled_returns.push((w[1] - w[0]) / w[0]);
                }
            }
        }
    }

    if pooled_returns.len() < crate::tail_metrics::MIN_RETURN_OBSERVATIONS {
        entry.tail_metrics = None;
        return;
    }

    // Build a synthetic equity curve from pooled returns
    let mut eq = Vec::with_capacity(pooled_returns.len() + 1);
    eq.push(100_000.0);
    for &r in &pooled_returns {
        eq.push(eq.last().unwrap() * (1.0 + r));
    }

    entry.tail_metrics = Some(compute_tail_metrics(&eq));
}

/// Recompute aggregated stickiness from per-symbol stickiness data.
fn recompute_stickiness(entry: &mut CrossSymbolEntry) {
    if entry.symbol_stickiness.is_empty() {
        entry.avg_stickiness = None;
        return;
    }

    let sticks: Vec<&StickinessMetrics> = entry.symbol_stickiness.values().collect();
    let n = sticks.len() as f64;

    let avg_median_holding_bars = sticks.iter().map(|s| s.median_holding_bars).sum::<f64>() / n;
    let worst_median_holding_bars = sticks
        .iter()
        .map(|s| s.median_holding_bars)
        .fold(0.0_f64, f64::max);
    let avg_exit_trigger_rate = sticks.iter().map(|s| s.exit_trigger_rate).sum::<f64>() / n;
    let worst_exit_trigger_rate = sticks
        .iter()
        .map(|s| s.exit_trigger_rate)
        .fold(1.0_f64, f64::min);
    let avg_reference_chase_ratio =
        sticks.iter().map(|s| s.reference_chase_ratio).sum::<f64>() / n;
    let worst_reference_chase_ratio = sticks
        .iter()
        .map(|s| s.reference_chase_ratio)
        .fold(0.0_f64, f64::max);

    let is_pathological = sticks.iter().any(|s| is_pathological_stickiness(s));

    entry.avg_stickiness = Some(AggregatedStickiness {
        avg_median_holding_bars,
        worst_median_holding_bars,
        avg_exit_trigger_rate,
        worst_exit_trigger_rate,
        avg_reference_chase_ratio,
        worst_reference_chase_ratio,
        symbol_count: sticks.len(),
        is_pathological,
    });
}

/// Geometric mean of CAGR values across symbols.
///
/// Uses log-return aggregation for robustness when any symbol has negative returns:
/// `exp(mean(ln(1 + total_return_i))) - 1` annualized.
///
/// Falls back to simple geometric mean when all returns are positive:
/// `(product of (1 + cagr_i))^(1/n) - 1`.
fn geo_mean_cagr(metrics: &[&PerformanceMetrics]) -> f64 {
    if metrics.is_empty() {
        return 0.0;
    }

    let n = metrics.len() as f64;
    let has_negative = metrics.iter().any(|m| m.total_return < 0.0);

    if has_negative {
        // Log-return based: handles negative total returns
        let log_sum: f64 = metrics
            .iter()
            .map(|m| {
                let ratio = 1.0 + m.total_return;
                if ratio > 0.0 {
                    ratio.ln()
                } else {
                    // Total loss: cap at ln(0.001) to avoid -inf
                    0.001_f64.ln()
                }
            })
            .sum();

        let mean_log = log_sum / n;
        // This gives geometric mean of total return ratio, not annualized CAGR.
        // For cross-symbol comparison this is the right metric since all symbols
        // share the same date range in a YOLO session.
        mean_log.exp() - 1.0
    } else {
        // Standard geometric mean of (1 + cagr) values
        let product: f64 = metrics.iter().map(|m| 1.0 + m.cagr).product();
        if product <= 0.0 {
            return 0.0;
        }
        product.powf(1.0 / n) - 1.0
    }
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use trendlab_core::fingerprint::ComponentConfig;

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

    fn make_metrics(sharpe: f64, cagr: f64, total_return: f64, max_dd: f64) -> PerformanceMetrics {
        PerformanceMetrics {
            total_return,
            cagr,
            sharpe,
            sortino: sharpe * 1.2,
            calmar: if max_dd < 0.0 {
                cagr / max_dd.abs()
            } else {
                0.0
            },
            max_drawdown: max_dd,
            win_rate: 0.55,
            profit_factor: 1.5,
            trade_count: 20,
            turnover: 2.0,
            max_consecutive_wins: 4,
            max_consecutive_losses: 3,
            avg_losing_streak: 1.5,
        }
    }

    fn make_equity(n: usize, daily_return: f64) -> Vec<f64> {
        let mut eq = Vec::with_capacity(n);
        eq.push(100_000.0);
        for _ in 1..n {
            eq.push(eq.last().unwrap() * (1.0 + daily_return));
        }
        eq
    }

    fn ts() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn single_symbol_identity() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let config = make_config("donchian", 50.0);
        let metrics = make_metrics(1.5, 0.10, 0.10, -0.08);
        let eq = make_equity(253, 0.001);

        lb.insert_result("SPY", metrics.clone(), &eq, &config, "s1", 0, ts());

        assert_eq!(lb.len(), 1);
        let entry = lb.entries().values().next().unwrap();
        assert_eq!(entry.symbol_count, 1);
        assert!((entry.avg_sharpe - 1.5).abs() < 1e-10);
        assert!((entry.min_sharpe - 1.5).abs() < 1e-10);
        assert!((entry.max_sharpe - 1.5).abs() < 1e-10);
        assert!((entry.hit_rate - 1.0).abs() < 1e-10);
        assert!((entry.worst_max_drawdown - (-0.08)).abs() < 1e-10);
    }

    #[test]
    fn two_symbols_correct_aggregates() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let config = make_config("donchian", 50.0);
        let eq = make_equity(253, 0.001);

        lb.insert_result(
            "SPY",
            make_metrics(2.0, 0.15, 0.15, -0.05),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "QQQ",
            make_metrics(1.0, 0.05, 0.05, -0.12),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );

        let entry = lb.entries().values().next().unwrap();
        assert_eq!(entry.symbol_count, 2);
        assert!((entry.avg_sharpe - 1.5).abs() < 1e-10);
        assert!((entry.min_sharpe - 1.0).abs() < 1e-10);
        assert!((entry.max_sharpe - 2.0).abs() < 1e-10);
        // Both profitable → hit rate = 1.0
        assert!((entry.hit_rate - 1.0).abs() < 1e-10);
        // Worst drawdown is -0.12
        assert!((entry.worst_max_drawdown - (-0.12)).abs() < 1e-10);
    }

    #[test]
    fn hit_rate_partial_profitability() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let config = make_config("bollinger", 20.0);
        let eq = make_equity(253, 0.001);

        lb.insert_result(
            "SPY",
            make_metrics(1.5, 0.10, 0.10, -0.05),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "QQQ",
            make_metrics(-0.5, -0.03, -0.03, -0.15),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "IWM",
            make_metrics(0.8, 0.05, 0.05, -0.10),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );

        let entry = lb.entries().values().next().unwrap();
        // 2 out of 3 profitable
        assert!((entry.hit_rate - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn geo_mean_cagr_all_positive() {
        let metrics = vec![
            make_metrics(1.0, 0.10, 0.10, -0.05),
            make_metrics(1.0, 0.20, 0.20, -0.05),
        ];
        let refs: Vec<&PerformanceMetrics> = metrics.iter().collect();
        let gm = geo_mean_cagr(&refs);
        // Geometric mean of (1.1, 1.2) = sqrt(1.32) - 1 ≈ 0.1489
        let expected = (1.1 * 1.2_f64).sqrt() - 1.0;
        assert!(
            (gm - expected).abs() < 1e-6,
            "Geo mean CAGR expected {expected}, got {gm}"
        );
    }

    #[test]
    fn geo_mean_cagr_with_negative() {
        let metrics = vec![
            make_metrics(1.0, 0.10, 0.10, -0.05),
            make_metrics(-0.5, -0.05, -0.05, -0.15),
        ];
        let refs: Vec<&PerformanceMetrics> = metrics.iter().collect();
        let gm = geo_mean_cagr(&refs);
        // Uses log-return method when any symbol is negative
        let expected = ((1.10_f64.ln() + 0.95_f64.ln()) / 2.0).exp() - 1.0;
        assert!(
            (gm - expected).abs() < 1e-6,
            "Geo mean CAGR expected {expected}, got {gm}"
        );
    }

    #[test]
    fn catastrophic_flag_triggers() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let config = make_config("donchian", 50.0);
        let eq = make_equity(253, 0.001);

        lb.insert_result(
            "SPY",
            make_metrics(1.0, 0.10, 0.10, -0.05),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "JUNK",
            make_metrics(-2.0, -0.60, -0.60, -0.70), // CAGR < -0.5 threshold
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );

        let entry = lb.entries().values().next().unwrap();
        assert!(entry.has_catastrophic);
    }

    #[test]
    fn catastrophic_flag_not_set_above_threshold() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let config = make_config("donchian", 50.0);
        let eq = make_equity(253, 0.001);

        lb.insert_result(
            "SPY",
            make_metrics(1.0, 0.10, 0.10, -0.05),
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "QQQ",
            make_metrics(-0.5, -0.10, -0.10, -0.20), // CAGR = -0.10 > -0.5
            &eq,
            &config,
            "s1",
            0,
            ts(),
        );

        let entry = lb.entries().values().next().unwrap();
        assert!(!entry.has_catastrophic);
    }

    #[test]
    fn different_configs_are_separate_entries() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let config1 = make_config("donchian", 50.0);
        let config2 = make_config("donchian", 100.0);
        let eq = make_equity(253, 0.001);

        lb.insert_result(
            "SPY",
            make_metrics(1.5, 0.10, 0.10, -0.05),
            &eq,
            &config1,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "SPY",
            make_metrics(2.0, 0.15, 0.15, -0.03),
            &eq,
            &config2,
            "s1",
            1,
            ts(),
        );

        assert_eq!(lb.len(), 2);
    }

    #[test]
    fn incremental_equals_batch() {
        // Insert symbols one-at-a-time and verify same result as inserting all at once
        let config = make_config("bollinger", 20.0);
        let eq = make_equity(253, 0.001);
        let m1 = make_metrics(2.0, 0.15, 0.15, -0.05);
        let m2 = make_metrics(1.0, 0.05, 0.05, -0.12);

        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        lb.insert_result("SPY", m1.clone(), &eq, &config, "s1", 0, ts());
        lb.insert_result("QQQ", m2.clone(), &eq, &config, "s1", 0, ts());

        let entry = lb.entries().values().next().unwrap();
        let avg = entry.avg_sharpe;
        let min = entry.min_sharpe;
        let max = entry.max_sharpe;
        let hit = entry.hit_rate;

        // Expected values from batch
        assert!((avg - 1.5).abs() < 1e-10);
        assert!((min - 1.0).abs() < 1e-10);
        assert!((max - 2.0).abs() < 1e-10);
        assert!((hit - 1.0).abs() < 1e-10);
    }

    #[test]
    fn trim_removes_worst() {
        let mut lb = CrossSymbolLeaderboard::new(2, -0.5);
        let eq = make_equity(253, 0.001);

        let c1 = make_config("donchian", 50.0);
        let c2 = make_config("donchian", 100.0);
        let c3 = make_config("bollinger", 20.0);

        lb.insert_result(
            "SPY",
            make_metrics(1.0, 0.05, 0.05, -0.05),
            &eq,
            &c1,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "SPY",
            make_metrics(2.0, 0.10, 0.10, -0.05),
            &eq,
            &c2,
            "s1",
            1,
            ts(),
        );
        lb.insert_result(
            "SPY",
            make_metrics(3.0, 0.15, 0.15, -0.05),
            &eq,
            &c3,
            "s1",
            2,
            ts(),
        );

        lb.trim();

        assert_eq!(lb.len(), 2);
        // The entry with avg_sharpe = 1.0 should be removed
        let ranked = lb.get_ranked(RankingMetric::AvgSharpe);
        assert!((ranked[0].avg_sharpe - 3.0).abs() < 1e-10);
        assert!((ranked[1].avg_sharpe - 2.0).abs() < 1e-10);
    }

    #[test]
    fn get_ranked_sorts_correctly() {
        let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
        let eq = make_equity(253, 0.001);

        let c1 = make_config("donchian", 50.0);
        let c2 = make_config("donchian", 100.0);
        let c3 = make_config("bollinger", 20.0);

        lb.insert_result(
            "SPY",
            make_metrics(1.0, 0.05, 0.05, -0.05),
            &eq,
            &c1,
            "s1",
            0,
            ts(),
        );
        lb.insert_result(
            "SPY",
            make_metrics(3.0, 0.15, 0.15, -0.05),
            &eq,
            &c2,
            "s1",
            1,
            ts(),
        );
        lb.insert_result(
            "SPY",
            make_metrics(2.0, 0.10, 0.10, -0.05),
            &eq,
            &c3,
            "s1",
            2,
            ts(),
        );

        let ranked = lb.get_ranked(RankingMetric::AvgSharpe);
        assert!((ranked[0].avg_sharpe - 3.0).abs() < 1e-10);
        assert!((ranked[1].avg_sharpe - 2.0).abs() < 1e-10);
        assert!((ranked[2].avg_sharpe - 1.0).abs() < 1e-10);
    }

    #[test]
    fn empty_leaderboard() {
        let lb = CrossSymbolLeaderboard::new(100, -0.5);
        assert!(lb.is_empty());
        assert_eq!(lb.len(), 0);
        assert!(lb.get_ranked(RankingMetric::AvgSharpe).is_empty());
    }
}
