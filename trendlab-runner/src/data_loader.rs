//! Bar loading and data resolution for the runner.
//!
//! Given a list of symbols, loads bars from the Parquet cache and returns
//! aligned bar data. Implements the fallback policy:
//! 1. If cached data exists → use it
//! 2. If not cached and provider available → auto-download and cache
//! 3. If no data and `--synthetic` → generate synthetic bars (tagged)
//! 4. Otherwise → fail with a clear error
//!
//! Synthetic data is a developer-only debug mode. Results produced on
//! synthetic data are tagged and cannot enter the all-time leaderboard.

use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use thiserror::Error;
use trendlab_core::data::{
    align::{align_symbols, AlignedData},
    cache::ParquetCache,
    provider::{DataError, DataProvider, DataSource, DownloadProgress, RawBar},
};

/// Errors from the data loading layer.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error(
        "no cached data for '{symbol}' and no network access (use --synthetic for synthetic data)"
    )]
    NoCachedDataOffline { symbol: String },

    #[error("no cached data for '{symbol}' and download failed: {reason}")]
    DownloadFailed { symbol: String, reason: String },

    #[error("data error: {0}")]
    Data(#[from] DataError),
}

/// Options controlling how bars are loaded.
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Start date for bars.
    pub start: NaiveDate,
    /// End date for bars.
    pub end: NaiveDate,
    /// If true, never make network requests.
    pub offline: bool,
    /// If true, generate synthetic bars when real data is unavailable.
    /// Mutually exclusive with `offline` (enforced at call site).
    pub synthetic: bool,
    /// Force re-download even if cached.
    pub force: bool,
}

/// Result of loading bars, including data source provenance.
#[derive(Debug)]
pub struct LoadedData {
    /// Aligned bar data for all symbols.
    pub aligned: AlignedData,
    /// Data source per symbol.
    pub sources: HashMap<String, DataSource>,
    /// Dataset hash for fingerprinting (BLAKE3 over all bar data).
    pub dataset_hash: String,
    /// Whether any symbol used synthetic data.
    pub has_synthetic: bool,
}

/// Load bars for a set of symbols from the cache, with fallback to download or synthetic.
///
/// This is the primary entry point for the runner to get bar data.
pub fn load_bars(
    symbols: &[&str],
    cache: &ParquetCache,
    provider: Option<&dyn DataProvider>,
    progress: Option<&dyn DownloadProgress>,
    opts: &LoadOptions,
) -> Result<LoadedData, LoadError> {
    let mut all_bars: HashMap<String, Vec<RawBar>> = HashMap::new();
    let mut sources: HashMap<String, DataSource> = HashMap::new();
    let mut has_synthetic = false;

    for (i, symbol) in symbols.iter().enumerate() {
        let total = symbols.len();

        // Step 1: Try cache
        if !opts.force {
            if let Ok(bars) = cache.load(symbol) {
                if let Some(p) = progress {
                    p.on_start(symbol, i, total);
                    p.on_complete(symbol, i, total, &Ok(()));
                }
                all_bars.insert(symbol.to_string(), bars);
                sources.insert(symbol.to_string(), DataSource::Cache);
                continue;
            }
        }

        // Step 2: Try download (if not offline and provider available)
        if !opts.offline {
            if let Some(prov) = provider {
                if prov.is_available() {
                    if let Some(p) = progress {
                        p.on_start(symbol, i, total);
                    }
                    match prov.fetch(symbol, opts.start, opts.end) {
                        Ok(fetch_result) => {
                            let ingested = trendlab_core::data::ingest::ingest(fetch_result.bars)?;
                            cache.write(symbol, &ingested.bars)?;
                            if let Some(p) = progress {
                                p.on_complete(symbol, i, total, &Ok(()));
                            }
                            all_bars.insert(symbol.to_string(), ingested.bars);
                            sources.insert(symbol.to_string(), DataSource::YahooFinance);
                            continue;
                        }
                        Err(e) => {
                            if let Some(p) = progress {
                                p.on_complete(symbol, i, total, &Err(e));
                            }
                            // Fall through to synthetic or error
                        }
                    }
                }
            }
        }

        // Step 3: Synthetic fallback (if enabled)
        if opts.synthetic {
            eprintln!(
                "WARNING: generating synthetic data for {symbol} — results will be tagged as synthetic"
            );
            let bars = generate_synthetic_bars(symbol, opts.start, opts.end);
            all_bars.insert(symbol.to_string(), bars);
            sources.insert(symbol.to_string(), DataSource::Synthetic);
            has_synthetic = true;
            continue;
        }

        // Step 4: Fail
        if opts.offline {
            return Err(LoadError::NoCachedDataOffline {
                symbol: symbol.to_string(),
            });
        }
        return Err(LoadError::DownloadFailed {
            symbol: symbol.to_string(),
            reason: "data not cached and download failed".into(),
        });
    }

    // Align all symbols to a common timeline
    let aligned = align_symbols(all_bars);

    // Compute deterministic dataset hash
    let dataset_hash = compute_dataset_hash(&aligned);

    if let Some(p) = progress {
        let succeeded = sources.len();
        let failed = symbols.len() - succeeded;
        p.on_batch_complete(succeeded, failed, symbols.len());
    }

    Ok(LoadedData {
        aligned,
        sources,
        dataset_hash,
        has_synthetic,
    })
}

/// Compute a deterministic BLAKE3 hash over all bar data.
///
/// The hash covers dates and all OHLCV values in sorted symbol order,
/// ensuring it's identical regardless of HashMap iteration order.
fn compute_dataset_hash(aligned: &AlignedData) -> String {
    let mut hasher = blake3::Hasher::new();

    // Sort symbols for deterministic ordering
    let mut symbols: Vec<&String> = aligned.bars.keys().collect();
    symbols.sort();

    for symbol in &symbols {
        hasher.update(symbol.as_bytes());
        if let Some(bars) = aligned.bars.get(*symbol) {
            for bar in bars {
                hasher.update(bar.date.to_string().as_bytes());
                hasher.update(&bar.open.to_le_bytes());
                hasher.update(&bar.high.to_le_bytes());
                hasher.update(&bar.low.to_le_bytes());
                hasher.update(&bar.close.to_le_bytes());
                hasher.update(&bar.volume.to_le_bytes());
                hasher.update(&bar.adj_close.to_le_bytes());
            }
        }
    }

    hasher.finalize().to_hex().to_string()
}

/// Generate synthetic bars for testing/development.
///
/// Produces a simple random walk from a starting price of 100.0.
/// These are clearly fake and tagged as synthetic.
fn generate_synthetic_bars(symbol: &str, start: NaiveDate, end: NaiveDate) -> Vec<RawBar> {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    // Deterministic seed from symbol name
    let seed_bytes = blake3::hash(symbol.as_bytes());
    let seed: [u8; 32] = *seed_bytes.as_bytes();
    let mut rng = StdRng::from_seed(seed);

    let mut bars = Vec::new();
    let mut price = 100.0_f64;
    let mut current = start;

    while current <= end {
        // Skip weekends (simple heuristic)
        let weekday = current.weekday();
        if weekday == chrono::Weekday::Sat || weekday == chrono::Weekday::Sun {
            current += chrono::Duration::days(1);
            continue;
        }

        let daily_return: f64 = rng.gen_range(-0.03..0.03);
        let open = price;
        let close = price * (1.0 + daily_return);
        let high = open.max(close) * (1.0 + rng.gen_range(0.0..0.01));
        let low = open.min(close) * (1.0 - rng.gen_range(0.0..0.01));
        let volume = rng.gen_range(500_000..5_000_000u64);

        bars.push(RawBar {
            date: current,
            open,
            high,
            low,
            close,
            volume,
            adj_close: close,
        });

        price = close;
        current += chrono::Duration::days(1);
    }

    bars
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_cache_dir() -> std::path::PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("trendlab_runner_test_{}_{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_bars() -> Vec<RawBar> {
        vec![
            RawBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1000,
                adj_close: 101.0,
            },
            RawBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
                open: 101.0,
                high: 103.0,
                low: 100.0,
                close: 102.0,
                volume: 1100,
                adj_close: 102.0,
            },
        ]
    }

    #[test]
    fn load_from_cache_succeeds() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);
        cache.write("SPY", &sample_bars()).unwrap();

        let opts = LoadOptions {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            offline: false,
            synthetic: false,
            force: false,
        };

        let loaded = load_bars(&["SPY"], &cache, None, None, &opts).unwrap();

        assert_eq!(loaded.aligned.bars["SPY"].len(), 2);
        assert_eq!(loaded.sources["SPY"], DataSource::Cache);
        assert!(!loaded.has_synthetic);
        assert!(!loaded.dataset_hash.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn offline_no_cache_fails_without_synthetic() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        let opts = LoadOptions {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            offline: true,
            synthetic: false,
            force: false,
        };

        let result = load_bars(&["SPY"], &cache, None, None, &opts);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no cached data"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_fallback_produces_tagged_data() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        let opts = LoadOptions {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2024, 3, 31).unwrap(),
            offline: false,
            synthetic: true,
            force: false,
        };

        let loaded = load_bars(&["FAKE"], &cache, None, None, &opts).unwrap();

        assert!(loaded.has_synthetic);
        assert_eq!(loaded.sources["FAKE"], DataSource::Synthetic);
        assert!(!loaded.aligned.bars["FAKE"].is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_data_is_deterministic() {
        let bars1 = generate_synthetic_bars(
            "SPY",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        );
        let bars2 = generate_synthetic_bars(
            "SPY",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        );

        assert_eq!(bars1.len(), bars2.len());
        for (a, b) in bars1.iter().zip(bars2.iter()) {
            assert_eq!(a.date, b.date);
            assert_eq!(a.close, b.close);
        }
    }

    #[test]
    fn different_symbols_get_different_synthetic_data() {
        let spy = generate_synthetic_bars(
            "SPY",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        );
        let qqq = generate_synthetic_bars(
            "QQQ",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        );

        // Same date range but different symbols → different prices
        assert_eq!(spy.len(), qqq.len());
        assert_ne!(spy[0].close, qqq[0].close);
    }

    #[test]
    fn dataset_hash_is_deterministic() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);
        cache.write("SPY", &sample_bars()).unwrap();

        let opts = LoadOptions {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            offline: false,
            synthetic: false,
            force: false,
        };

        let loaded1 = load_bars(&["SPY"], &cache, None, None, &opts).unwrap();
        let loaded2 = load_bars(&["SPY"], &cache, None, None, &opts).unwrap();

        assert_eq!(loaded1.dataset_hash, loaded2.dataset_hash);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_symbol_alignment_via_loader() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        let spy_bars = vec![
            RawBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1000,
                adj_close: 101.0,
            },
            RawBar {
                date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(),
                open: 101.0,
                high: 103.0,
                low: 100.0,
                close: 102.0,
                volume: 1100,
                adj_close: 102.0,
            },
        ];
        let qqq_bars = vec![RawBar {
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 200.0,
            high: 204.0,
            low: 198.0,
            close: 202.0,
            volume: 2000,
            adj_close: 202.0,
        }];

        cache.write("SPY", &spy_bars).unwrap();
        cache.write("QQQ", &qqq_bars).unwrap();

        let opts = LoadOptions {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            offline: false,
            synthetic: false,
            force: false,
        };

        let loaded = load_bars(&["SPY", "QQQ"], &cache, None, None, &opts).unwrap();

        // Both should have 2 bars (aligned to union of dates)
        assert_eq!(loaded.aligned.bars["SPY"].len(), 2);
        assert_eq!(loaded.aligned.bars["QQQ"].len(), 2);

        // QQQ bar on 2024-01-03 should be void (NaN)
        assert!(loaded.aligned.bars["QQQ"][1].close.is_nan());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
