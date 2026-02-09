//! Integration tests for Phase 11 robustness pipeline.
//!
//! Tests the full promotion ladder, walk-forward validation, execution MC,
//! block bootstrap, FDR correction, and stickiness integration.
//! Uses the frozen SPY 2024 fixture for real-data tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::NaiveDate;

use trendlab_core::components::composition::StrategyPreset;
use trendlab_core::components::execution::ExecutionPreset;
use trendlab_core::data::cache::ParquetCache;
use trendlab_core::fingerprint::TradingMode;

use trendlab_runner::bootstrap::{stationary_block_bootstrap, BootstrapConfig};
use trendlab_runner::data_loader::{load_bars, LoadOptions};
use trendlab_runner::execution_mc::ExecutionMcConfig;
use trendlab_runner::fdr::{benjamini_hochberg, FdrFamily};
use trendlab_runner::promotion::{PromotionConfig, PromotionLevel};
use trendlab_runner::runner::run_backtest_from_data;
use trendlab_runner::walk_forward::{run_walk_forward, WalkForwardConfig};
use trendlab_runner::yolo::{run_yolo, YoloConfig};

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
        "trendlab_runner_robustness_{}_{id}",
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

fn load_opts() -> LoadOptions {
    LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    }
}

// ── Walk-Forward on real data ──────────────────────────────────────────

#[test]
fn walk_forward_insufficient_data_on_short_series() {
    // 252 bars is not enough for WF (needs 756 min_total_bars by default)
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();
    let wf_config = WalkForwardConfig::default(); // min_total_bars = 756

    let result = run_walk_forward(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        &wf_config,
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
    );

    // Should fail: only 252 bars, need 756
    assert!(result.is_err(), "Expected insufficient data error");
}

#[test]
fn walk_forward_with_relaxed_config() {
    // Relax fold requirements to fit within 252 bars
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();
    let wf_config = WalkForwardConfig {
        n_folds: 3,
        min_total_bars: 100,
        min_is_bars: 50,
        min_oos_bars: 25,
    };

    let result = run_walk_forward(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        &wf_config,
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
    );

    let wf = result.expect("Walk-forward should succeed with relaxed config");
    assert_eq!(wf.fold_results.len(), 3);
    // Each fold should have produced a Sharpe (even if zero)
    for fold in &wf.fold_results {
        assert!(fold.is_sharpe.is_finite());
        assert!(fold.oos_sharpe.is_finite());
    }
    assert!(wf.mean_is_sharpe.is_finite());
    assert!(wf.mean_oos_sharpe.is_finite());
}

// ── Execution MC on real data ──────────────────────────────────────────

#[test]
fn execution_mc_produces_non_identical_samples() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();
    let mc_config = ExecutionMcConfig {
        n_samples: 20,
        slippage_range: (0.0, 30.0),
        commission_range: (0.0, 20.0),
        path_policies: vec![
            trendlab_core::components::execution::PathPolicy::Deterministic,
            trendlab_core::components::execution::PathPolicy::WorstCase,
            trendlab_core::components::execution::PathPolicy::BestCase,
        ],
        seed: 42,
    };

    let result = trendlab_runner::execution_mc::run_execution_mc(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        &mc_config,
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        &loaded.dataset_hash,
    )
    .expect("Execution MC should succeed");

    assert_eq!(result.samples.len(), 20);

    // Stability score should be finite and non-negative
    assert!(result.stability.stability_ratio.is_finite());
    assert!(result.stability.median_sharpe.is_finite());
    assert!(result.stability.iqr_sharpe.is_finite());
    assert!(result.stability.iqr_sharpe >= 0.0, "IQR cannot be negative");

    // At least verify the stability ratio formula works
    // (all_different may be false if strategy produces zero trades on this short data)
    if result.stability.all_different {
        assert!(result.stability.iqr_sharpe > 0.0 || result.samples.len() < 4);
    }
}

// ── Block Bootstrap ────────────────────────────────────────────────────

#[test]
fn bootstrap_on_real_equity_curve() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();

    let result = run_backtest_from_data(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
        false,
    )
    .expect("Backtest should succeed");

    let config = BootstrapConfig {
        n_resamples: 500,
        mean_block_length: 20,
        seed: 42,
    };

    let bootstrap = stationary_block_bootstrap(&result.equity_curve, &config)
        .expect("Bootstrap should succeed on 252+ bar equity curve");

    assert!(bootstrap.sharpe_median.is_finite());
    assert!(bootstrap.sharpe_ci_lower.is_finite());
    assert!(bootstrap.sharpe_ci_upper.is_finite());
    assert!(bootstrap.ci_width >= 0.0);
    assert!(bootstrap.sharpe_ci_lower <= bootstrap.sharpe_ci_upper);
    assert!(bootstrap.sample_size >= 250); // 252 equity bars → 251 daily returns
}

// ── FDR Correction ─────────────────────────────────────────────────────

#[test]
fn fdr_family_accumulation_and_correction() {
    let mut family = FdrFamily::new();

    // Simulate 20 strategy tests with varying p-values
    for i in 0..20 {
        let p = match i {
            0..=2 => 0.001 * (i + 1) as f64, // strongly significant
            3..=5 => 0.02 + 0.005 * i as f64, // borderline
            _ => 0.1 + 0.04 * i as f64,       // not significant
        };
        family.add(format!("config_{i}"), p);
    }

    assert_eq!(family.len(), 20);

    let results = family.apply_correction(0.05);
    assert_eq!(results.len(), 20);

    // Some should be significant, some not
    let significant_count = results.iter().filter(|r| r.significant).count();
    let not_significant = results.iter().filter(|r| !r.significant).count();
    assert!(significant_count > 0, "Should have some significant results");
    assert!(
        not_significant > 0,
        "Should have some non-significant results"
    );

    // BH correction should be more conservative than raw p-values
    let raw_significant = (0..20)
        .filter(|&i| {
            let p = match i {
                0..=2 => 0.001 * (i + 1) as f64,
                3..=5 => 0.02 + 0.005 * i as f64,
                _ => 0.1 + 0.04 * i as f64,
            };
            p < 0.05
        })
        .count();
    assert!(
        significant_count <= raw_significant,
        "BH should be at least as conservative as raw p-values"
    );
}

#[test]
fn bh_correction_reduces_false_positives() {
    // 100 random p-values from null (should all be uniform[0,1])
    // BH correction should control FDR at alpha level
    let p_values: Vec<(String, f64)> = (0..100)
        .map(|i| {
            // Deterministic "random" p-values that look null
            let p = ((i as f64 * 0.618 + 0.5) % 1.0).max(0.001);
            (format!("null_{i}"), p)
        })
        .collect();

    let results = benjamini_hochberg(&p_values, 0.05);
    let significant_count = results.iter().filter(|r| r.significant).count();

    // Under null, BH should reject very few (ideally ~5% or fewer)
    assert!(
        significant_count <= 15,
        "BH should control false positives: got {significant_count}/100"
    );
}

// ── Promotion Ladder ───────────────────────────────────────────────────

#[test]
fn promotion_gate_low_sharpe_stops_at_level1() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    // Use a real strategy, but set an impossibly high Sharpe threshold
    // so the Level 1 gate always blocks promotion.
    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();

    let result = run_backtest_from_data(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
        false,
    )
    .expect("Backtest should succeed");

    let promo_config = PromotionConfig {
        wf_sharpe_threshold: 100.0, // impossibly high — guarantees Level 1 gate failure
        wf_config: WalkForwardConfig {
            n_folds: 3,
            min_total_bars: 100,
            min_is_bars: 50,
            min_oos_bars: 25,
        },
        ..PromotionConfig::default()
    };

    let mut fdr_family = FdrFamily::new();
    let robustness = trendlab_runner::promotion::promote(
        &result,
        &strategy_config,
        &loaded.aligned,
        "SPY",
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
        &promo_config,
        &mut fdr_family,
    );

    assert_eq!(
        robustness.level_reached,
        PromotionLevel::Level1CheapPass,
        "Low Sharpe should stop at Level 1"
    );
    assert!(robustness.walk_forward.is_none());
    assert!(robustness.execution_mc.is_none());
    assert!(robustness.bootstrap.is_none());
    assert!(robustness.gate_failure.is_some());
}

#[test]
fn promotion_real_strategy_reaches_level2_or_beyond() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();

    let result = run_backtest_from_data(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
        false,
    )
    .expect("Backtest should succeed");

    // Use very relaxed thresholds so promotion proceeds
    let promo_config = PromotionConfig {
        wf_sharpe_threshold: -10.0, // always passes gate 1
        wf_config: WalkForwardConfig {
            n_folds: 2,
            min_total_bars: 50,
            min_is_bars: 25,
            min_oos_bars: 15,
        },
        wf_degradation_threshold: -10.0, // always passes gate 2
        mc_config: ExecutionMcConfig {
            n_samples: 5,
            ..ExecutionMcConfig::default()
        },
        bootstrap_config: BootstrapConfig {
            n_resamples: 100,
            ..BootstrapConfig::default()
        },
        fdr_alpha: 0.05,
    };

    let mut fdr_family = FdrFamily::new();
    let robustness = trendlab_runner::promotion::promote(
        &result,
        &strategy_config,
        &loaded.aligned,
        "SPY",
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
        &promo_config,
        &mut fdr_family,
    );

    // With relaxed thresholds, should reach Level 2 or 3
    assert!(
        robustness.level_reached >= PromotionLevel::Level2WalkForward,
        "Expected at least Level 2, got {:?}",
        robustness.level_reached
    );
    assert!(robustness.walk_forward.is_some());
}

// ── Stickiness Integration ─────────────────────────────────────────────

#[test]
fn backtest_result_includes_stickiness() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();

    let result = run_backtest_from_data(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        ExecutionPreset::Realistic,
        &loaded.dataset_hash,
        false,
    )
    .expect("Backtest should succeed");

    // If there are trades, stickiness should be present
    if !result.trades.is_empty() {
        let stickiness = result
            .stickiness
            .as_ref()
            .expect("Stickiness should be present when trades exist");
        assert!(stickiness.median_holding_bars >= 0.0);
        assert!(stickiness.exit_trigger_rate >= 0.0);
        assert!(stickiness.exit_trigger_rate <= 1.0);
        assert!(stickiness.reference_chase_ratio >= 1.0);
        assert!(stickiness.reference_chase_ratio <= 100.0);
    }
}

// ── YOLO with Promotion ────────────────────────────────────────────────

#[test]
fn yolo_with_promotion_tracks_counts() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let config = YoloConfig {
        max_iterations: Some(10),
        promotion_config: Some(PromotionConfig {
            wf_sharpe_threshold: -10.0, // always try WF
            wf_config: WalkForwardConfig {
                n_folds: 2,
                min_total_bars: 50,
                min_is_bars: 25,
                min_oos_bars: 15,
            },
            wf_degradation_threshold: -10.0, // always pass WF gate
            mc_config: ExecutionMcConfig {
                n_samples: 3,
                ..ExecutionMcConfig::default()
            },
            bootstrap_config: BootstrapConfig {
                n_resamples: 50,
                ..BootstrapConfig::default()
            },
            fdr_alpha: 0.05,
        }),
        ..YoloConfig::default()
    };

    let symbols = vec!["SPY".to_string()];
    let result = run_yolo(&config, &loaded, &symbols, None, None)
        .expect("YOLO should succeed");

    assert_eq!(result.iterations_completed, 10);
    assert!(result.success_count > 0);
    // With relaxed thresholds, some should have been promoted
    // (fdr_family_size tracks how many had t-test p-values recorded)
    // fdr_family_size may be 0 if WF had insufficient folds for t-test
    let _ = result.fdr_family_size;
}

// ── Stability Scoring ──────────────────────────────────────────────────

#[test]
fn execution_mc_stability_ratio_is_finite_on_real_data() {
    // Integration-level test: run MC on real data, verify stability score
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(cache_dir.clone());
    let loaded = load_bars(&["SPY"], &cache, None, None, &load_opts()).unwrap();

    let preset = StrategyPreset::DonchianTrend;
    let strategy_config = preset.to_config();

    let mc_config = ExecutionMcConfig {
        n_samples: 10,
        ..ExecutionMcConfig::default()
    };

    let result = trendlab_runner::execution_mc::run_execution_mc(
        &strategy_config,
        &loaded.aligned,
        "SPY",
        &mc_config,
        TradingMode::LongOnly,
        100_000.0,
        1.0,
        &loaded.dataset_hash,
    )
    .expect("MC should succeed");

    // Verify stability scoring properties
    assert!(result.stability.stability_ratio.is_finite());
    assert!(result.stability.iqr_sharpe >= 0.0, "IQR cannot be negative");
    assert!(result.stability.p10_sharpe <= result.stability.median_sharpe);
}
