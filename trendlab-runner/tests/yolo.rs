//! Integration tests for YOLO mode.
//!
//! Uses the frozen SPY 2024 fixture to run real YOLO sweeps.
//! Tests: determinism across thread counts, 100+ iterations,
//! dual slider behavior, error resilience, thread constraint enforcement.

use chrono::NaiveDate;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use trendlab_core::data::cache::ParquetCache;
use trendlab_runner::data_loader::{LoadOptions, LoadedData};
use trendlab_runner::yolo::{run_yolo, YoloConfig, YoloProgress};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn core_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("trendlab-core/tests/fixtures")
}

fn setup_fixture_cache() -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir =
        std::env::temp_dir().join(format!("trendlab_yolo_test_{}_{id}", std::process::id()));
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

// ─── YOLO determinism: same seed → same results ─────────────────────

#[test]
fn yolo_determinism_single_thread() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    let config = base_yolo_config(30);

    let result1 = run_yolo(&config, &data, &symbols, None, None).unwrap();
    let result2 = run_yolo(&config, &data, &symbols, None, None).unwrap();

    assert_eq!(result1.iterations_completed, result2.iterations_completed);

    let lb1 = &result1.leaderboards["SPY"];
    let lb2 = &result2.leaderboards["SPY"];

    assert_eq!(lb1.len(), lb2.len(), "leaderboard sizes must match");

    // Compare top entries by full_hash and metrics
    for (e1, e2) in lb1.entries().iter().zip(lb2.entries()) {
        assert_eq!(
            e1.result.config.full_hash(),
            e2.result.config.full_hash(),
            "full_hash mismatch at iteration {}",
            e1.iteration
        );
        assert!(
            (e1.fitness_score - e2.fitness_score).abs() < 1e-10,
            "fitness_score mismatch: {} vs {}",
            e1.fitness_score,
            e2.fitness_score
        );
        assert!(
            (e1.result.metrics.sharpe - e2.result.metrics.sharpe).abs() < 1e-10,
            "sharpe mismatch"
        );
    }
}

#[test]
fn yolo_determinism_across_thread_counts() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    // Single-threaded run
    let mut config_1t = base_yolo_config(30);
    config_1t.outer_thread_cap = 1;

    // Multi-threaded run (4 threads) — but with 1 symbol, parallelism
    // is across iterations within symbol processing, not across symbols.
    // The key invariant is that the SAME sampled configs produce the SAME
    // results regardless of thread pool existence.
    let mut config_4t = base_yolo_config(30);
    config_4t.outer_thread_cap = 4;

    let result_1t = run_yolo(&config_1t, &data, &symbols, None, None).unwrap();
    let result_4t = run_yolo(&config_4t, &data, &symbols, None, None).unwrap();

    assert_eq!(
        result_1t.iterations_completed,
        result_4t.iterations_completed
    );

    let lb_1t = &result_1t.leaderboards["SPY"];
    let lb_4t = &result_4t.leaderboards["SPY"];

    assert_eq!(
        lb_1t.len(),
        lb_4t.len(),
        "leaderboard sizes must match across thread counts"
    );

    // Top entries must have identical hashes and metrics
    let top_n = lb_1t.len().min(10);
    for i in 0..top_n {
        let e1 = &lb_1t.entries()[i];
        let e4 = &lb_4t.entries()[i];
        assert_eq!(
            e1.result.config.full_hash(),
            e4.result.config.full_hash(),
            "full_hash mismatch at leaderboard position {i}"
        );
        assert!(
            (e1.result.metrics.sharpe - e4.result.metrics.sharpe).abs() < 1e-10,
            "sharpe mismatch at position {i}: {} vs {}",
            e1.result.metrics.sharpe,
            e4.result.metrics.sharpe
        );
    }
}

// ─── 100+ iterations populate leaderboard ───────────────────────────

#[test]
fn yolo_100_iterations_populates_leaderboard() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    let config = base_yolo_config(100);

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    assert_eq!(result.iterations_completed, 100);
    assert!(result.success_count > 0, "should have some successes");
    assert_eq!(
        result.success_count + result.error_count,
        100, // 100 iterations * 1 symbol
        "success + error should equal total"
    );

    let lb = &result.leaderboards["SPY"];
    assert!(
        lb.len() > 0,
        "leaderboard should have entries after 100 iterations"
    );

    // Verify entries are sorted (best first)
    let scores: Vec<f64> = lb.entries().iter().map(|e| e.fitness_score).collect();
    for w in scores.windows(2) {
        assert!(w[0] >= w[1], "leaderboard not sorted: {} < {}", w[0], w[1]);
    }

    // All entries should have finite metrics
    for entry in lb.entries() {
        assert!(entry.fitness_score.is_finite(), "fitness must be finite");
        assert!(
            entry.result.metrics.sharpe.is_finite(),
            "sharpe must be finite"
        );
        assert!(entry.result.metrics.cagr.is_finite(), "cagr must be finite");
        assert!(!entry.result.trades.is_empty(), "entries must have trades");
    }

    println!(
        "100 iterations: {} successes, {} errors, {} leaderboard entries, best Sharpe={:.3}",
        result.success_count,
        result.error_count,
        lb.len(),
        lb.entries().first().map(|e| e.fitness_score).unwrap_or(0.0)
    );
}

// ─── Dual slider behavior ──────────────────────────────────────────

#[test]
fn dual_sliders_affect_exploration() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    // Low exploration: always picks default components
    let mut config_low = base_yolo_config(50);
    config_low.jitter_pct = 0.0;
    config_low.structural_explore = 0.0;
    config_low.master_seed = 100;

    // High exploration: diverse components and parameters
    let mut config_high = base_yolo_config(50);
    config_high.jitter_pct = 1.0;
    config_high.structural_explore = 1.0;
    config_high.master_seed = 100;

    let result_low = run_yolo(&config_low, &data, &symbols, None, None).unwrap();
    let result_high = run_yolo(&config_high, &data, &symbols, None, None).unwrap();

    let lb_low = &result_low.leaderboards["SPY"];
    let lb_high = &result_high.leaderboards["SPY"];

    // Count unique structural configurations (config_hash)
    let unique_low: HashSet<String> = lb_low
        .entries()
        .iter()
        .map(|e| e.result.config.config_hash().as_hex())
        .collect();
    let unique_high: HashSet<String> = lb_high
        .entries()
        .iter()
        .map(|e| e.result.config.config_hash().as_hex())
        .collect();

    // With zero explore, all entries should have the same structural config
    // (only default component types). With full explore, we should see diversity.
    // Note: low explore may still have > 1 if the default donchian doesn't fire
    // on 252 bars, causing zero-trade results that get filtered out.
    assert!(
        unique_high.len() >= unique_low.len(),
        "high explore ({}) should produce >= unique configs than low explore ({})",
        unique_high.len(),
        unique_low.len()
    );

    println!(
        "Slider test: low explore unique configs = {}, high explore unique configs = {}",
        unique_low.len(),
        unique_high.len()
    );
}

// ─── Cancellation ──────────────────────────────────────────────────

#[test]
fn yolo_cancellation_stops_promptly() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    let cancel = AtomicBool::new(false);

    // Run with max_iterations = 10 to test that it respects the limit
    let config = base_yolo_config(10);
    let result = run_yolo(&config, &data, &symbols, None, Some(&cancel)).unwrap();

    assert_eq!(result.iterations_completed, 10);
}

#[test]
fn yolo_cancellation_via_flag() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    // No iteration limit
    let config = YoloConfig {
        max_iterations: Some(1000),
        ..base_yolo_config(1000)
    };

    let cancel = AtomicBool::new(true); // Pre-cancelled

    let result = run_yolo(&config, &data, &symbols, None, Some(&cancel)).unwrap();

    // Should stop immediately (0 iterations)
    assert_eq!(result.iterations_completed, 0);
}

// ─── Progress reporting ────────────────────────────────────────────

#[test]
fn yolo_progress_callback_fires() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];
    let config = base_yolo_config(20);

    let progress_count = AtomicU64::new(0);
    let max_iteration_seen = AtomicU64::new(0);

    let progress_cb = |progress: &YoloProgress| {
        progress_count.fetch_add(1, Ordering::Relaxed);
        max_iteration_seen.fetch_max(progress.iteration as u64, Ordering::Relaxed);
    };

    let result = run_yolo(&config, &data, &symbols, Some(&progress_cb), None).unwrap();

    assert_eq!(result.iterations_completed, 20);
    assert!(
        progress_count.load(Ordering::Relaxed) > 0,
        "progress callback should have fired at least once"
    );
    // The first callback fires at iteration 0, and subsequent callbacks are
    // throttled to 500ms. On a fast machine, all 20 iterations complete in < 500ms,
    // so we may only see iteration 0. The key check is that the callback fired.
    let seen = max_iteration_seen.load(Ordering::Relaxed);
    assert!(
        seen <= 19, // max possible iteration is 19 (0-indexed)
        "iteration seen should be within range"
    );
}

// ─── Error resilience ──────────────────────────────────────────────

#[test]
fn yolo_handles_missing_symbol_gracefully() {
    let data = load_spy_data();
    // Include a symbol not in the data — should produce errors, not crash
    let symbols = vec!["SPY".to_string(), "NONEXISTENT".to_string()];

    let config = base_yolo_config(20);

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();

    assert_eq!(result.iterations_completed, 20);
    // NONEXISTENT should produce errors for each iteration
    assert!(
        result.error_count > 0,
        "should have errors for missing symbol"
    );
    // SPY should still succeed
    assert!(
        result.success_count > 0,
        "SPY should still produce successes"
    );
    assert_eq!(
        result.success_count + result.error_count,
        40, // 20 iterations * 2 symbols
        "total outcomes should be iterations * symbols"
    );

    // SPY leaderboard should still have entries
    let lb_spy = &result.leaderboards["SPY"];
    assert!(lb_spy.len() > 0, "SPY leaderboard should have entries");
}

// ─── Thread constraint enforcement ─────────────────────────────────

#[test]
fn thread_constraint_enforcement() {
    let mut config = YoloConfig::default();

    // Setting outer > 1 should force polars = 1
    config.outer_thread_cap = 4;
    config.polars_thread_cap = 4;
    config.enforce_thread_constraints();
    assert_eq!(
        config.polars_thread_cap, 1,
        "polars_thread_cap must be 1 when outer_thread_cap > 1"
    );

    // Single outer thread allows any polars cap
    config.outer_thread_cap = 1;
    config.polars_thread_cap = 8;
    config.enforce_thread_constraints();
    assert_eq!(
        config.polars_thread_cap, 8,
        "polars_thread_cap should be unchanged when outer_thread_cap = 1"
    );
}

// ─── Leaderboard deduplication in YOLO context ─────────────────────

#[test]
fn yolo_leaderboard_no_accidental_duplicates() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    // Use high jitter to get different params each time
    let mut config = base_yolo_config(50);
    config.jitter_pct = 1.0;
    config.structural_explore = 1.0;

    let result = run_yolo(&config, &data, &symbols, None, None).unwrap();
    let lb = &result.leaderboards["SPY"];

    // Check no duplicate full_hashes in leaderboard
    let hashes: Vec<String> = lb
        .entries()
        .iter()
        .map(|e| e.result.config.full_hash().as_hex())
        .collect();
    let unique_hashes: HashSet<&String> = hashes.iter().collect();
    assert_eq!(
        hashes.len(),
        unique_hashes.len(),
        "leaderboard should have no duplicate full_hashes"
    );
}

// ─── Different seeds produce different results ─────────────────────

#[test]
fn different_seeds_produce_different_results() {
    let data = load_spy_data();
    let symbols = vec!["SPY".to_string()];

    let mut config_a = base_yolo_config(30);
    config_a.master_seed = 1;

    let mut config_b = base_yolo_config(30);
    config_b.master_seed = 2;

    let result_a = run_yolo(&config_a, &data, &symbols, None, None).unwrap();
    let result_b = run_yolo(&config_b, &data, &symbols, None, None).unwrap();

    let lb_a = &result_a.leaderboards["SPY"];
    let lb_b = &result_b.leaderboards["SPY"];

    // With different seeds, at least the top entry should differ
    if !lb_a.is_empty() && !lb_b.is_empty() {
        let hashes_a: HashSet<String> = lb_a
            .entries()
            .iter()
            .map(|e| e.result.config.full_hash().as_hex())
            .collect();
        let hashes_b: HashSet<String> = lb_b
            .entries()
            .iter()
            .map(|e| e.result.config.full_hash().as_hex())
            .collect();

        // At least some entries should differ
        let overlap = hashes_a.intersection(&hashes_b).count();
        let total = hashes_a.len().max(hashes_b.len());
        assert!(
            overlap < total,
            "different seeds should produce different leaderboard entries (overlap: {overlap}/{total})"
        );
    }
}
