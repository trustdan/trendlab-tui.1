//! Integration tests for the data pipeline using the frozen SPY fixture.

use chrono::Datelike;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use trendlab_core::data::{cache::ParquetCache, provider::RawBar, DataError};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_spy_fixture() -> Vec<RawBar> {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    // Load the frozen fixture by writing it into a temp cache and reading back
    let cache_dir =
        std::env::temp_dir().join(format!("trendlab_fixture_test_{}_{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);

    // Create Hive-partitioned structure
    let sym_dir = cache_dir.join("symbol=SPY");
    std::fs::create_dir_all(&sym_dir).unwrap();
    std::fs::copy(
        fixture_dir().join("spy_2024.parquet"),
        sym_dir.join("2024.parquet"),
    )
    .unwrap();

    // Write a minimal meta.json
    let meta = r#"{"symbol":"SPY","start_date":"2024-01-02","end_date":"2024-12-31","bar_count":252,"data_hash":"fixture","source":"fixture","cached_at":"2024-01-01T00:00:00"}"#;
    std::fs::write(sym_dir.join("meta.json"), meta).unwrap();

    let cache = ParquetCache::new(&cache_dir);
    let bars = cache.load("SPY").unwrap();

    let _ = std::fs::remove_dir_all(&cache_dir);
    bars
}

#[test]
fn frozen_fixture_loads_real_spy_data() {
    let bars = load_spy_fixture();

    // Verify we have a full year of trading days
    assert!(
        bars.len() >= 250,
        "expected ~252 trading days, got {}",
        bars.len()
    );
    assert!(bars.len() <= 253);

    // Verify dates are in 2024
    assert_eq!(bars[0].date.year(), 2024);
    assert_eq!(bars.last().unwrap().date.year(), 2024);

    // Verify dates are sorted ascending
    for window in bars.windows(2) {
        assert!(window[0].date < window[1].date);
    }

    // Verify prices are realistic (SPY was ~470-600 in 2024)
    for bar in &bars {
        assert!(bar.close > 100.0, "SPY price too low: {}", bar.close);
        assert!(bar.close < 1000.0, "SPY price too high: {}", bar.close);
        assert!(bar.high >= bar.low);
        assert!(bar.volume > 0);
    }
}

#[test]
fn frozen_fixture_bars_are_not_void() {
    let bars = load_spy_fixture();

    let void_count = bars.iter().filter(|b| b.open.is_nan()).count();
    assert_eq!(void_count, 0, "frozen fixture should have no void bars");
}

#[test]
fn cache_miss_returns_clear_error() {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache_dir =
        std::env::temp_dir().join(format!("trendlab_miss_test_{}_{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cache = ParquetCache::new(&cache_dir);
    let result = cache.load("NONEXISTENT");

    match result {
        Err(DataError::NoCachedData { symbol }) => {
            assert_eq!(symbol, "NONEXISTENT");
        }
        other => panic!("expected NoCachedData error, got: {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn alignment_with_fixture_data() {
    let bars = load_spy_fixture();

    // Simulate alignment with a symbol that has fewer bars
    let subset: Vec<RawBar> = bars[10..20].to_vec();

    let mut input = HashMap::new();
    input.insert("SPY".into(), bars);
    input.insert("SUBSET".into(), subset);

    let aligned = trendlab_core::data::align::align_symbols(input);

    // Both symbols should have the same number of bars (union of dates)
    assert_eq!(aligned.bars["SPY"].len(), aligned.bars["SUBSET"].len());

    // SUBSET should have void bars where SPY has real bars
    let void_count = aligned.bars["SUBSET"]
        .iter()
        .filter(|b| b.open.is_nan())
        .count();
    assert!(void_count > 200, "SUBSET should have many void bars");
}
