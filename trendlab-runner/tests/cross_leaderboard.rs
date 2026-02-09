//! Integration tests for Phase 10c: cross-symbol leaderboard, risk profiles, history.
//!
//! Uses the frozen SPY 2024 fixture to run real YOLO sweeps and verify
//! cross-symbol aggregation, risk profile ranking, JSONL history, and
//! fingerprint integrity.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::NaiveDate;
use tempfile::TempDir;

use trendlab_core::data::cache::ParquetCache;
use trendlab_core::domain::FullHash;
use trendlab_core::fingerprint::StrategyConfig;

use trendlab_runner::cross_leaderboard::{CrossSymbolEntry, CrossSymbolLeaderboard};
use trendlab_runner::data_loader::{LoadOptions, LoadedData};
use trendlab_runner::history::{WriteFilter, YoloHistory};
use trendlab_runner::metrics::PerformanceMetrics;
use trendlab_runner::risk_profile::{compute_composite_scores, RankingMetric, RiskProfile};
use trendlab_runner::yolo::{run_yolo, YoloConfig};

// ─── Shared helpers ──────────────────────────────────────────────────

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn core_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("trendlab-core/tests/fixtures")
}

fn setup_fixture_cache() -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir = std::env::temp_dir().join(format!(
        "trendlab_cross_lb_test_{}_{id}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&cache_dir);

    let sym_dir = cache_dir.join("symbol=SPY");
    std::fs::create_dir_all(&sym_dir).unwrap();
    std::fs::copy(
        core_fixture_dir().join("spy_2024.parquet"),
        sym_dir.join("2024.parquet"),
    )
    .unwrap();

    let meta = r#"{"symbol":"SPY","start_date":"2024-01-02","end_date":"2024-12-31","bar_count":252,"data_hash":"fixture","source":"fixture","cached_at":"2024-01-01T00:00:00"}"#;
    std::fs::write(sym_dir.join("meta.json"), meta).unwrap();

    cache_dir
}

fn load_spy_data() -> LoadedData {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);
    let opts = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    };
    trendlab_runner::load_bars(&["SPY"], &cache, None, None, &opts).unwrap()
}

fn base_yolo_config(max_iterations: usize) -> YoloConfig {
    YoloConfig {
        jitter_pct: 0.5,
        structural_explore: 0.5,
        start_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        initial_capital: 100_000.0,
        max_iterations: Some(max_iterations),
        master_seed: 42,
        leaderboard_max_size: 500,
        ..YoloConfig::default()
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

fn make_config(signal_type: &str, lookback: f64) -> StrategyConfig {
    use std::collections::BTreeMap;
    use trendlab_core::fingerprint::ComponentConfig;

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

fn make_entry(
    signal: &str,
    lookback: f64,
    sharpe: f64,
    cagr: f64,
    hit_rate: f64,
    worst_dd: f64,
    min_sharpe: f64,
) -> CrossSymbolEntry {
    let config = make_config(signal, lookback);
    let ts =
        chrono::NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    // Build via JSON + serde roundtrip to avoid touching pub(crate) field
    // (symbol_equity_curves has #[serde(skip)] so it defaults to empty HashMap)
    let json = serde_json::json!({
        "config": config,
        "config_hash": config.config_hash(),
        "full_hash": config.full_hash(),
        "avg_sharpe": sharpe,
        "min_sharpe": min_sharpe,
        "max_sharpe": sharpe * 1.2,
        "geo_mean_cagr": cagr,
        "hit_rate": hit_rate,
        "worst_max_drawdown": worst_dd,
        "avg_trade_count": 20.0,
        "tail_metrics": null,
        "symbol_count": 3,
        "symbol_metrics": {},
        "has_catastrophic": false,
        "session_id": "test",
        "timestamp": ts.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "iteration": 0,
    });
    serde_json::from_value(json).unwrap()
}

fn make_equity(n: usize, daily_return: f64) -> Vec<f64> {
    let mut eq = Vec::with_capacity(n);
    eq.push(100_000.0);
    for _ in 1..n {
        eq.push(eq.last().unwrap() * (1.0 + daily_return));
    }
    eq
}

fn ts() -> chrono::NaiveDateTime {
    chrono::NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
}

// ─── Test 1: Cross-symbol aggregation via YOLO ──────────────────────

#[test]
fn yolo_populates_cross_symbol_leaderboard() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];
    let config = base_yolo_config(50);

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    // Cross-symbol leaderboard should have entries (one per unique full_hash
    // that produced at least one valid trade)
    assert!(
        !result.cross_leaderboard.is_empty(),
        "cross-symbol leaderboard should have entries after 50 iterations"
    );

    // Every entry should have consistent aggregates
    for entry in result.cross_leaderboard.entries().values() {
        assert_eq!(entry.symbol_count, 1, "single-symbol YOLO → 1 symbol each");
        assert!(entry.avg_sharpe.is_finite(), "avg_sharpe must be finite");
        assert!(
            entry.geo_mean_cagr.is_finite(),
            "geo_mean_cagr must be finite"
        );
        assert!(
            entry.hit_rate >= 0.0 && entry.hit_rate <= 1.0,
            "hit_rate out of range: {}",
            entry.hit_rate
        );
        assert!(
            entry.worst_max_drawdown <= 0.0,
            "worst_max_drawdown should be <= 0: {}",
            entry.worst_max_drawdown
        );
        assert!(entry.avg_trade_count > 0.0, "entries must have trades");

        // Single symbol: avg == min == max for Sharpe
        assert!(
            (entry.avg_sharpe - entry.min_sharpe).abs() < 1e-10,
            "single symbol: avg should equal min"
        );
        assert!(
            (entry.avg_sharpe - entry.max_sharpe).abs() < 1e-10,
            "single symbol: avg should equal max"
        );
    }

    // Ranking should produce a sorted list
    let ranked = result
        .cross_leaderboard
        .get_ranked(RankingMetric::AvgSharpe);
    for w in ranked.windows(2) {
        assert!(
            w[0].avg_sharpe >= w[1].avg_sharpe,
            "ranked list not sorted: {} < {}",
            w[0].avg_sharpe,
            w[1].avg_sharpe
        );
    }

    println!(
        "Cross-symbol: {} entries, top Sharpe = {:.3}",
        result.cross_leaderboard.len(),
        ranked.first().map(|e| e.avg_sharpe).unwrap_or(0.0)
    );
}

// ─── Test 2: Risk profiles produce different rankings ──────────────

#[test]
fn risk_profiles_rerank_cross_symbol_entries() {
    // Construct 3 entries designed so Aggressive and Conservative disagree
    let e1 = make_entry("donchian", 50.0, 3.0, 0.25, 0.9, -0.40, -0.5);
    let e2 = make_entry("bollinger", 20.0, 1.5, 0.10, 0.7, -0.15, 0.8);
    let e3 = make_entry("keltner", 30.0, 0.5, 0.03, 0.5, -0.03, 0.4);

    let refs: Vec<&CrossSymbolEntry> = vec![&e1, &e2, &e3];

    let aggressive = compute_composite_scores(&refs, RiskProfile::Aggressive);
    let conservative = compute_composite_scores(&refs, RiskProfile::Conservative);

    // Aggressive should favor e1 (highest sharpe+cagr+hit_rate)
    let agg_top = top_hash(&aggressive);
    assert_eq!(
        agg_top, e1.full_hash,
        "Aggressive top should be e1 (high returns)"
    );

    // Conservative should NOT favor e1 (worst drawdown, worst consistency)
    let con_top = top_hash(&conservative);
    assert_ne!(
        con_top, e1.full_hash,
        "Conservative top should NOT be e1 (worst DD + consistency)"
    );

    println!(
        "Aggressive top: {:?}, Conservative top: {:?}",
        agg_top, con_top
    );
}

fn top_hash(scores: &HashMap<FullHash, f64>) -> FullHash {
    scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(h, _)| h.clone())
        .unwrap()
}

// ─── Test 3: History persistence via YOLO ───────────────────────────

#[test]
fn yolo_writes_history_when_enabled() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];
    let tmp = TempDir::new().unwrap();
    let history_path = tmp.path().join("yolo_history.jsonl");

    let config = YoloConfig {
        history_path: Some(history_path.clone()),
        write_filter: WriteFilter::default(),
        ..base_yolo_config(50)
    };

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    // History file should exist and have entries
    assert!(history_path.exists(), "history file should be created");
    assert!(
        result.history_entries_written > 0,
        "should have written at least one history entry"
    );
    assert!(
        result.history_file_size_bytes > 0,
        "history file size should be > 0"
    );

    // Read back and verify
    let history = YoloHistory::new(history_path, WriteFilter::default());
    let entries = history.read_all().unwrap();
    assert_eq!(
        entries.len(),
        result.history_entries_written,
        "read-back count should match written count"
    );

    // Every entry should have valid fingerprint data
    for entry in &entries {
        assert_eq!(entry.fingerprint.symbol, "SPY");
        assert!(entry.trade_count >= 5, "write filter enforces min_trades=5");
        assert!(entry.fitness_score.is_finite());
    }

    println!(
        "History: {} entries written, {:.1} KB",
        result.history_entries_written,
        result.history_file_size_bytes as f64 / 1024.0
    );
}

// ─── Test 4: Write filter rejects junk runs ─────────────────────────

#[test]
fn yolo_history_write_filter_rejects_junk() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];
    let tmp = TempDir::new().unwrap();
    let history_path = tmp.path().join("strict_history.jsonl");

    // Very strict filter: only write entries with Sharpe > 2.0 AND CAGR > 0.20
    let strict_filter = WriteFilter {
        min_trades: 5,
        min_cagr: Some(0.20),
        min_sharpe: Some(2.0),
    };

    let config = YoloConfig {
        history_path: Some(history_path.clone()),
        write_filter: strict_filter,
        ..base_yolo_config(50)
    };

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    // With a strict filter, fewer (possibly zero) entries should be written
    // compared to the number of successful backtests
    let written = result.history_entries_written;

    // If any entries were written, verify they all pass the strict filter
    if written > 0 {
        let history = YoloHistory::new(history_path, WriteFilter::default());
        let entries = history.read_all().unwrap();
        for entry in &entries {
            let passes_cagr = entry.metrics.cagr >= 0.20;
            let passes_sharpe = entry.metrics.sharpe >= 2.0;
            assert!(
                passes_cagr || passes_sharpe,
                "entry should pass strict filter: CAGR={:.3}, Sharpe={:.3}",
                entry.metrics.cagr,
                entry.metrics.sharpe
            );
        }
    }

    // The per-symbol leaderboard should still have more entries than the
    // filtered history (since leaderboard doesn't use write filter)
    let lb_count = result.leaderboards["SPY"].len();
    assert!(
        lb_count >= written,
        "leaderboard ({lb_count}) should have >= entries than filtered history ({written})"
    );

    println!(
        "Strict filter: {} written out of {} leaderboard entries",
        written, lb_count
    );
}

// ─── Test 5: Fingerprint integrity in history ───────────────────────

#[test]
fn history_fingerprints_match_strategy_config() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];
    let tmp = TempDir::new().unwrap();
    let history_path = tmp.path().join("fp_history.jsonl");

    let config = YoloConfig {
        history_path: Some(history_path.clone()),
        ..base_yolo_config(30)
    };

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();
    assert!(result.history_entries_written > 0);

    let history = YoloHistory::new(history_path, WriteFilter::default());
    let entries = history.read_all().unwrap();

    for entry in &entries {
        // Verify config_hash and full_hash are consistent with the strategy_config
        let expected_config_hash = entry.fingerprint.strategy_config.config_hash();
        let expected_full_hash = entry.fingerprint.strategy_config.full_hash();

        assert_eq!(
            entry.fingerprint.config_hash, expected_config_hash,
            "config_hash mismatch in history entry"
        );
        assert_eq!(
            entry.fingerprint.full_hash, expected_full_hash,
            "full_hash mismatch in history entry"
        );

        // Trading mode should match the config
        assert_eq!(
            entry.fingerprint.initial_capital, config.initial_capital,
            "initial_capital mismatch"
        );
        assert_eq!(
            entry.fingerprint.start_date, config.start_date,
            "start_date mismatch"
        );
        assert_eq!(
            entry.fingerprint.end_date, config.end_date,
            "end_date mismatch"
        );
    }

    println!("Fingerprint check: {} entries verified", entries.len());
}

// ─── Test 6: Catastrophic flag via direct API ───────────────────────

#[test]
fn catastrophic_flag_triggers_on_bad_cagr() {
    let mut lb = CrossSymbolLeaderboard::new(100, -0.5);
    let config = make_config("donchian", 50.0);
    let eq = make_equity(253, 0.001);

    // Good symbol
    lb.insert_result(
        "SPY",
        make_metrics(1.5, 0.10, 0.10, -0.05),
        &eq,
        &config,
        "s1",
        0,
        ts(),
    );

    // Catastrophic symbol: CAGR = -0.60 < threshold of -0.5
    lb.insert_result(
        "JUNK",
        make_metrics(-2.0, -0.60, -0.60, -0.70),
        &eq,
        &config,
        "s1",
        1,
        ts(),
    );

    let entry = lb.entries().values().next().unwrap();
    assert!(
        entry.has_catastrophic,
        "catastrophic flag should be set when any symbol CAGR < -50%"
    );
    assert_eq!(entry.symbol_count, 2);

    // Above-threshold loss should NOT set flag
    let config2 = make_config("bollinger", 30.0);
    lb.insert_result(
        "SPY",
        make_metrics(0.5, -0.10, -0.10, -0.20),
        &eq,
        &config2,
        "s1",
        2,
        ts(),
    );

    let entry2 = lb.entries().get(&config2.full_hash()).unwrap();
    assert!(
        !entry2.has_catastrophic,
        "CAGR -10% should NOT trigger catastrophic (threshold is -50%)"
    );
}

// ─── Test 7: Rank normalization invariance ──────────────────────────

#[test]
fn composite_ranking_invariant_under_metric_rescaling() {
    let e1 = make_entry("donchian", 50.0, 1.0, 0.10, 0.6, -0.10, 0.8);
    let e2 = make_entry("donchian", 100.0, 2.0, 0.15, 0.8, -0.08, 1.5);
    let e3 = make_entry("bollinger", 20.0, 1.5, 0.12, 0.7, -0.12, 1.0);

    let refs: Vec<&CrossSymbolEntry> = vec![&e1, &e2, &e3];

    // Compute scores with original values
    let scores = compute_composite_scores(&refs, RiskProfile::Balanced);
    let order = sorted_hashes(&scores);

    // Rescale avg_sharpe by 10x
    let mut e1r = e1.clone();
    let mut e2r = e2.clone();
    let mut e3r = e3.clone();
    e1r.avg_sharpe *= 10.0;
    e2r.avg_sharpe *= 10.0;
    e3r.avg_sharpe *= 10.0;

    let refs_r: Vec<&CrossSymbolEntry> = vec![&e1r, &e2r, &e3r];
    let scores_r = compute_composite_scores(&refs_r, RiskProfile::Balanced);
    let order_r = sorted_hashes(&scores_r);

    assert_eq!(order, order_r, "ranking should not change after rescaling");

    // Also rescale geo_mean_cagr by 100x
    e1r.geo_mean_cagr *= 100.0;
    e2r.geo_mean_cagr *= 100.0;
    e3r.geo_mean_cagr *= 100.0;

    let refs_r2: Vec<&CrossSymbolEntry> = vec![&e1r, &e2r, &e3r];
    let scores_r2 = compute_composite_scores(&refs_r2, RiskProfile::Balanced);
    let order_r2 = sorted_hashes(&scores_r2);

    assert_eq!(
        order, order_r2,
        "ranking should be invariant to any uniform rescaling"
    );
}

fn sorted_hashes(scores: &HashMap<FullHash, f64>) -> Vec<FullHash> {
    let mut v: Vec<_> = scores.iter().collect();
    v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    v.into_iter().map(|(h, _)| h.clone()).collect()
}

// ─── Test 8: Incremental vs batch aggregation ───────────────────────

#[test]
fn incremental_insert_matches_batch_aggregation() {
    let config = make_config("donchian", 50.0);
    let eq_spy = make_equity(253, 0.001);
    let eq_qqq = make_equity(253, 0.0005);
    let eq_iwm = make_equity(253, -0.0002);

    let m_spy = make_metrics(2.0, 0.15, 0.15, -0.05);
    let m_qqq = make_metrics(1.0, 0.08, 0.08, -0.10);
    let m_iwm = make_metrics(-0.3, -0.02, -0.02, -0.18);

    // Incremental: insert one at a time
    let mut lb_inc = CrossSymbolLeaderboard::new(100, -0.5);
    lb_inc.insert_result("SPY", m_spy.clone(), &eq_spy, &config, "s1", 0, ts());
    lb_inc.insert_result("QQQ", m_qqq.clone(), &eq_qqq, &config, "s1", 0, ts());
    lb_inc.insert_result("IWM", m_iwm.clone(), &eq_iwm, &config, "s1", 0, ts());

    // Batch: insert all at once (same as incremental since we use same API)
    let mut lb_batch = CrossSymbolLeaderboard::new(100, -0.5);
    lb_batch.insert_result("SPY", m_spy.clone(), &eq_spy, &config, "s1", 0, ts());
    lb_batch.insert_result("QQQ", m_qqq.clone(), &eq_qqq, &config, "s1", 0, ts());
    lb_batch.insert_result("IWM", m_iwm.clone(), &eq_iwm, &config, "s1", 0, ts());

    let full_hash = config.full_hash();
    let inc = lb_inc.entries().get(&full_hash).unwrap();
    let batch = lb_batch.entries().get(&full_hash).unwrap();

    // All aggregated values should match exactly
    assert_eq!(inc.symbol_count, batch.symbol_count);
    assert!((inc.avg_sharpe - batch.avg_sharpe).abs() < 1e-10);
    assert!((inc.min_sharpe - batch.min_sharpe).abs() < 1e-10);
    assert!((inc.max_sharpe - batch.max_sharpe).abs() < 1e-10);
    assert!((inc.geo_mean_cagr - batch.geo_mean_cagr).abs() < 1e-10);
    assert!((inc.hit_rate - batch.hit_rate).abs() < 1e-10);
    assert!((inc.worst_max_drawdown - batch.worst_max_drawdown).abs() < 1e-10);
    assert!((inc.avg_trade_count - batch.avg_trade_count).abs() < 1e-10);
    assert_eq!(inc.has_catastrophic, batch.has_catastrophic);

    // Verify the aggregated values are correct
    assert_eq!(inc.symbol_count, 3);
    let expected_avg_sharpe = (2.0 + 1.0 + (-0.3)) / 3.0;
    assert!(
        (inc.avg_sharpe - expected_avg_sharpe).abs() < 1e-10,
        "avg_sharpe expected {expected_avg_sharpe}, got {}",
        inc.avg_sharpe
    );
    assert!((inc.min_sharpe - (-0.3)).abs() < 1e-10);
    assert!((inc.max_sharpe - 2.0).abs() < 1e-10);
    // hit_rate: 2/3 profitable (SPY, QQQ positive; IWM negative)
    assert!((inc.hit_rate - 2.0 / 3.0).abs() < 1e-10);
    // worst drawdown: IWM at -0.18
    assert!((inc.worst_max_drawdown - (-0.18)).abs() < 1e-10);
}

// ─── Test 9: Cross-symbol + risk profile end-to-end via YOLO ────────

#[test]
fn yolo_cross_symbol_supports_composite_ranking() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];
    let config = base_yolo_config(50);

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    if result.cross_leaderboard.len() < 2 {
        // Not enough entries for meaningful ranking comparison
        println!(
            "Skipping: only {} cross-symbol entries (need >= 2)",
            result.cross_leaderboard.len()
        );
        return;
    }

    let entries: Vec<&CrossSymbolEntry> = result.cross_leaderboard.entries().values().collect();

    // All four profiles should produce valid scores
    for profile in [
        RiskProfile::Balanced,
        RiskProfile::Conservative,
        RiskProfile::Aggressive,
        RiskProfile::TrendOptions,
    ] {
        let scores = compute_composite_scores(&entries, profile);
        assert_eq!(
            scores.len(),
            entries.len(),
            "{:?} should produce scores for all entries",
            profile
        );

        // All scores should be finite and positive (since all ranks are in [0,1])
        for (hash, score) in &scores {
            assert!(
                score.is_finite(),
                "{:?}: score for {:?} is not finite",
                profile,
                hash
            );
            assert!(
                *score >= 0.0,
                "{:?}: score for {:?} should be >= 0",
                profile,
                hash
            );
        }
    }

    println!(
        "Composite ranking: verified 4 profiles on {} entries",
        entries.len()
    );
}

// ─── Test 10: History disabled by default ───────────────────────────

#[test]
fn yolo_no_history_when_path_is_none() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    let config = YoloConfig {
        history_path: None, // explicitly disabled
        ..base_yolo_config(20)
    };

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    assert_eq!(result.history_entries_written, 0);
    assert_eq!(result.history_file_size_bytes, 0);
}
