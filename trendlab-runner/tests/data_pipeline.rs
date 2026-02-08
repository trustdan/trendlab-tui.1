//! Integration tests for the runner's data pipeline.
//!
//! These tests verify that the runner can load real data from the Parquet
//! cache and run trivial backtests on it.

use chrono::NaiveDate;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use trendlab_core::data::{cache::ParquetCache, provider::DataSource};
use trendlab_runner::data_loader::{load_bars, LoadOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn core_fixture_dir() -> PathBuf {
    // The fixture lives in trendlab-core's test directory
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("trendlab-core/tests/fixtures")
}

fn setup_fixture_cache() -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir = std::env::temp_dir().join(format!(
        "trendlab_runner_pipeline_{}_{id}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&cache_dir);

    // Create Hive-partitioned structure from frozen fixture
    let sym_dir = cache_dir.join("symbol=SPY");
    std::fs::create_dir_all(&sym_dir).unwrap();
    std::fs::copy(
        core_fixture_dir().join("spy_2024.parquet"),
        sym_dir.join("2024.parquet"),
    )
    .unwrap();

    // Write a minimal meta.json
    let meta = r#"{"symbol":"SPY","start_date":"2024-01-02","end_date":"2024-12-31","bar_count":252,"data_hash":"fixture","source":"fixture","cached_at":"2024-01-01T00:00:00"}"#;
    std::fs::write(sym_dir.join("meta.json"), meta).unwrap();

    cache_dir
}

#[test]
fn runner_loads_real_spy_from_fixture() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);

    let opts = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    };

    let loaded = load_bars(&["SPY"], &cache, None, None, &opts).unwrap();

    // Verify we loaded real data
    assert_eq!(loaded.sources["SPY"], DataSource::Cache);
    assert!(!loaded.has_synthetic);

    // Verify we have a full year of trading days
    let spy_bars = &loaded.aligned.bars["SPY"];
    assert!(
        spy_bars.len() >= 250,
        "expected ~252 bars, got {}",
        spy_bars.len()
    );

    // Verify prices are realistic SPY prices (not synthetic)
    for bar in spy_bars {
        assert!(bar.close > 400.0, "SPY close too low: {}", bar.close);
        assert!(bar.close < 700.0, "SPY close too high: {}", bar.close);
    }

    // Verify dataset hash is deterministic
    let loaded2 = load_bars(&["SPY"], &cache, None, None, &opts).unwrap();
    assert_eq!(loaded.dataset_hash, loaded2.dataset_hash);
    assert!(!loaded.dataset_hash.is_empty());

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn runner_trivial_backtest_on_real_data() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);

    let opts = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    };

    let loaded = load_bars(&["SPY"], &cache, None, None, &opts).unwrap();
    let spy_bars = &loaded.aligned.bars["SPY"];

    // Trivial "backtest": buy first bar, sell last bar, compute PnL
    let entry_price = spy_bars.first().unwrap().open;
    let exit_price = spy_bars.last().unwrap().close;
    let pnl = exit_price - entry_price;

    // SPY went from ~472 to ~588 in 2024 — PnL should be positive and realistic
    assert!(entry_price > 400.0, "entry price too low: {}", entry_price);
    assert!(exit_price > entry_price, "SPY should have gone up in 2024");
    assert!(pnl > 50.0, "PnL should be meaningful: {pnl}");
    assert!(pnl < 200.0, "PnL should be realistic: {pnl}");

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn offline_no_cache_no_synthetic_fails_cleanly() {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir =
        std::env::temp_dir().join(format!("trendlab_runner_empty_{}_{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = ParquetCache::new(&cache_dir);

    let opts = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    };

    let result = load_bars(&["NONEXISTENT"], &cache, None, None, &opts);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no cached data"),
        "error should mention missing data: {err_msg}"
    );

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn synthetic_fallback_tagged_not_real() {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir =
        std::env::temp_dir().join(format!("trendlab_runner_synth_{}_{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = ParquetCache::new(&cache_dir);

    let opts = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 3, 31).unwrap(),
        offline: false,
        synthetic: true,
        force: false,
    };

    let loaded = load_bars(&["FAKE_TICKER"], &cache, None, None, &opts).unwrap();

    // Data is loaded but tagged as synthetic
    assert!(loaded.has_synthetic);
    assert_eq!(loaded.sources["FAKE_TICKER"], DataSource::Synthetic);

    // Synthetic prices should be around the 100.0 starting point, not real SPY-like
    let bars = &loaded.aligned.bars["FAKE_TICKER"];
    assert!(!bars.is_empty());
    assert!(
        bars[0].close < 200.0,
        "synthetic data should start near 100, got {}",
        bars[0].close
    );

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn dataset_hash_changes_with_different_data() {
    let cache_dir = setup_fixture_cache();
    let cache = ParquetCache::new(&cache_dir);

    let opts_real = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        offline: true,
        synthetic: false,
        force: false,
    };

    let loaded_real = load_bars(&["SPY"], &cache, None, None, &opts_real).unwrap();

    // Load synthetic data for a different symbol
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir2 =
        std::env::temp_dir().join(format!("trendlab_runner_hash_{}_{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir2);
    std::fs::create_dir_all(&cache_dir2).unwrap();
    let cache2 = ParquetCache::new(&cache_dir2);

    let opts_synth = LoadOptions {
        start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2024, 3, 31).unwrap(),
        offline: false,
        synthetic: true,
        force: false,
    };

    let loaded_synth = load_bars(&["FAKE"], &cache2, None, None, &opts_synth).unwrap();

    // Different data → different hash
    assert_ne!(loaded_real.dataset_hash, loaded_synth.dataset_hash);

    let _ = std::fs::remove_dir_all(&cache_dir);
    let _ = std::fs::remove_dir_all(&cache_dir2);
}
