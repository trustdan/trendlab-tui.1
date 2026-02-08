//! Parquet cache layer with Hive-style partitioning.
//!
//! Layout: `{cache_dir}/symbol={SYMBOL}/{year}.parquet`
//!
//! Features:
//! - Atomic writes (write to .tmp, rename into place)
//! - Incremental updates (download only missing date ranges)
//! - Integrity validation on load (schema check, row count > 0)
//! - Quarantine for corrupt files ({filename}.quarantined)
//! - Metadata sidecar per symbol (hash, date range, source)

use super::provider::{DataError, RawBar};
use chrono::{Datelike, NaiveDate};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata sidecar for a cached symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    pub symbol: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub bar_count: usize,
    pub data_hash: String,
    pub source: String,
    pub cached_at: chrono::NaiveDateTime,
}

/// The Parquet cache.
pub struct ParquetCache {
    cache_dir: PathBuf,
}

impl ParquetCache {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// Root directory of the cache.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Directory for a specific symbol: `{cache_dir}/symbol={SYMBOL}/`
    fn symbol_dir(&self, symbol: &str) -> PathBuf {
        self.cache_dir.join(format!("symbol={symbol}"))
    }

    /// Path to the Parquet file for a symbol+year: `{cache_dir}/symbol={SYMBOL}/{year}.parquet`
    fn year_path(&self, symbol: &str, year: i32) -> PathBuf {
        self.symbol_dir(symbol).join(format!("{year}.parquet"))
    }

    /// Path to the metadata sidecar for a symbol.
    fn meta_path(&self, symbol: &str) -> PathBuf {
        self.symbol_dir(symbol).join("meta.json")
    }

    /// Write bars for a symbol to the cache.
    ///
    /// Groups bars by year and writes one Parquet file per year.
    /// Writes are atomic: write to .tmp then rename.
    pub fn write(&self, symbol: &str, bars: &[RawBar]) -> Result<(), DataError> {
        if bars.is_empty() {
            return Err(DataError::CacheError("no bars to cache".into()));
        }

        let sym_dir = self.symbol_dir(symbol);
        fs::create_dir_all(&sym_dir)
            .map_err(|e| DataError::CacheError(format!("failed to create dir: {e}")))?;

        // Group bars by year
        let mut by_year: HashMap<i32, Vec<&RawBar>> = HashMap::new();
        for bar in bars {
            by_year.entry(bar.date.year()).or_default().push(bar);
        }

        // Write each year partition
        for (year, year_bars) in &by_year {
            let df = bars_to_dataframe(year_bars)?;
            let path = self.year_path(symbol, *year);
            let tmp_path = path.with_extension("parquet.tmp");

            write_parquet(&df, &tmp_path)?;

            // Atomic rename
            fs::rename(&tmp_path, &path).map_err(|e| {
                // Clean up temp file on rename failure
                let _ = fs::remove_file(&tmp_path);
                DataError::CacheError(format!("atomic rename failed: {e}"))
            })?;
        }

        // Write metadata sidecar
        let meta = CacheMeta {
            symbol: symbol.to_string(),
            start_date: bars.first().unwrap().date,
            end_date: bars.last().unwrap().date,
            bar_count: bars.len(),
            data_hash: blake3::hash(
                &serde_json::to_vec(bars)
                    .map_err(|e| DataError::CacheError(format!("hash serialization: {e}")))?,
            )
            .to_hex()
            .to_string(),
            source: "ingest".to_string(),
            cached_at: chrono::Local::now().naive_local(),
        };
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| DataError::CacheError(format!("meta serialization: {e}")))?;
        fs::write(self.meta_path(symbol), meta_json)
            .map_err(|e| DataError::CacheError(format!("meta write: {e}")))?;

        Ok(())
    }

    /// Load all cached bars for a symbol, sorted by date ascending.
    pub fn load(&self, symbol: &str) -> Result<Vec<RawBar>, DataError> {
        let sym_dir = self.symbol_dir(symbol);
        if !sym_dir.exists() {
            return Err(DataError::NoCachedData {
                symbol: symbol.to_string(),
            });
        }

        let mut all_bars = Vec::new();

        let entries =
            fs::read_dir(&sym_dir).map_err(|e| DataError::CacheError(format!("read dir: {e}")))?;

        for entry in entries {
            let entry = entry.map_err(|e| DataError::CacheError(format!("dir entry: {e}")))?;
            let path = entry.path();

            // Skip non-parquet files (meta.json, .quarantined, etc)
            if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
                continue;
            }

            match load_and_validate_parquet(&path) {
                Ok(bars) => all_bars.extend(bars),
                Err(e) => {
                    // Quarantine the corrupt file
                    let quarantine = path.with_extension("parquet.quarantined");
                    eprintln!(
                        "WARNING: quarantining corrupt cache file {}: {e}",
                        path.display()
                    );
                    let _ = fs::rename(&path, &quarantine);
                }
            }
        }

        if all_bars.is_empty() {
            return Err(DataError::NoCachedData {
                symbol: symbol.to_string(),
            });
        }

        all_bars.sort_by_key(|b| b.date);
        Ok(all_bars)
    }

    /// Check if a symbol has cached data and return its metadata.
    pub fn get_meta(&self, symbol: &str) -> Option<CacheMeta> {
        let meta_path = self.meta_path(symbol);
        let content = fs::read_to_string(meta_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Check which symbols have cached data, and their date ranges.
    pub fn status(&self, symbols: &[&str]) -> Vec<CacheStatus> {
        symbols
            .iter()
            .map(|sym| {
                let meta = self.get_meta(sym);
                CacheStatus {
                    symbol: sym.to_string(),
                    cached: meta.is_some(),
                    start_date: meta.as_ref().map(|m| m.start_date),
                    end_date: meta.as_ref().map(|m| m.end_date),
                    bar_count: meta.as_ref().map(|m| m.bar_count),
                }
            })
            .collect()
    }

    /// Check if cached data for a symbol covers the requested date range.
    pub fn covers_range(&self, symbol: &str, start: NaiveDate, end: NaiveDate) -> CoverageResult {
        match self.get_meta(symbol) {
            None => CoverageResult::NotCached,
            Some(meta) => {
                if meta.start_date <= start && meta.end_date >= end {
                    CoverageResult::FullyCovered
                } else {
                    CoverageResult::PartiallyCovered {
                        cached_start: meta.start_date,
                        cached_end: meta.end_date,
                    }
                }
            }
        }
    }
}

/// Cache status for a single symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatus {
    pub symbol: String,
    pub cached: bool,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub bar_count: Option<usize>,
}

/// How well the cache covers the requested date range.
#[derive(Debug, Clone, PartialEq)]
pub enum CoverageResult {
    NotCached,
    FullyCovered,
    PartiallyCovered {
        cached_start: NaiveDate,
        cached_end: NaiveDate,
    },
}

// ── Parquet I/O helpers ─────────────────────────────────────────────

/// Convert raw bars to a Polars DataFrame.
fn bars_to_dataframe(bars: &[&RawBar]) -> Result<DataFrame, DataError> {
    let dates: Vec<i32> = bars
        .iter()
        .map(|b| (b.date - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32)
        .collect();
    let opens: Vec<f64> = bars.iter().map(|b| b.open).collect();
    let highs: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let volumes: Vec<u64> = bars.iter().map(|b| b.volume).collect();
    let adj_closes: Vec<f64> = bars.iter().map(|b| b.adj_close).collect();

    DataFrame::new(vec![
        Column::new("date".into(), dates)
            .cast(&DataType::Date)
            .map_err(|e| DataError::ParquetError(format!("date cast: {e}")))?,
        Column::new("open".into(), opens),
        Column::new("high".into(), highs),
        Column::new("low".into(), lows),
        Column::new("close".into(), closes),
        Column::new("volume".into(), volumes),
        Column::new("adj_close".into(), adj_closes),
    ])
    .map_err(|e| DataError::ParquetError(format!("dataframe creation: {e}")))
}

/// Write a DataFrame to a Parquet file.
fn write_parquet(df: &DataFrame, path: &Path) -> Result<(), DataError> {
    let file =
        fs::File::create(path).map_err(|e| DataError::ParquetError(format!("create file: {e}")))?;
    ParquetWriter::new(file)
        .finish(&mut df.clone())
        .map_err(|e| DataError::ParquetError(format!("write parquet: {e}")))?;
    Ok(())
}

/// Load a Parquet file and validate its integrity.
fn load_and_validate_parquet(path: &Path) -> Result<Vec<RawBar>, DataError> {
    let file = fs::File::open(path).map_err(|e| DataError::ParquetError(format!("open: {e}")))?;
    let df = ParquetReader::new(file)
        .finish()
        .map_err(|e| DataError::ParquetError(format!("read: {e}")))?;

    // Validate: must have rows
    if df.height() == 0 {
        return Err(DataError::ValidationError("empty parquet file".into()));
    }

    // Validate: must have expected columns
    let expected_cols = [
        "date",
        "open",
        "high",
        "low",
        "close",
        "volume",
        "adj_close",
    ];
    for col_name in &expected_cols {
        if df.column(col_name).is_err() {
            return Err(DataError::ValidationError(format!(
                "missing column '{col_name}'"
            )));
        }
    }

    dataframe_to_bars(&df)
}

/// Convert a DataFrame back to RawBars.
fn dataframe_to_bars(df: &DataFrame) -> Result<Vec<RawBar>, DataError> {
    let map_err = |e: PolarsError| DataError::ParquetError(format!("column read: {e}"));

    let dates = df.column("date").map_err(map_err)?;
    let opens = df.column("open").map_err(map_err)?;
    let highs = df.column("high").map_err(map_err)?;
    let lows = df.column("low").map_err(map_err)?;
    let closes = df.column("close").map_err(map_err)?;
    let volumes = df.column("volume").map_err(map_err)?;
    let adj_closes = df.column("adj_close").map_err(map_err)?;

    let n = df.height();
    let mut bars = Vec::with_capacity(n);

    let date_ca = dates
        .date()
        .map_err(|e| DataError::ParquetError(format!("date column type: {e}")))?;
    let open_ca = opens
        .f64()
        .map_err(|e| DataError::ParquetError(format!("open column type: {e}")))?;
    let high_ca = highs
        .f64()
        .map_err(|e| DataError::ParquetError(format!("high column type: {e}")))?;
    let low_ca = lows
        .f64()
        .map_err(|e| DataError::ParquetError(format!("low column type: {e}")))?;
    let close_ca = closes
        .f64()
        .map_err(|e| DataError::ParquetError(format!("close column type: {e}")))?;
    let vol_ca = volumes
        .u64()
        .map_err(|e| DataError::ParquetError(format!("volume column type: {e}")))?;
    let adj_ca = adj_closes
        .f64()
        .map_err(|e| DataError::ParquetError(format!("adj_close column type: {e}")))?;

    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();

    for i in 0..n {
        let date_days = date_ca
            .get(i)
            .ok_or_else(|| DataError::ParquetError(format!("null date at row {i}")))?;
        let date = epoch + chrono::Duration::days(date_days as i64);

        bars.push(RawBar {
            date,
            open: open_ca.get(i).unwrap_or(f64::NAN),
            high: high_ca.get(i).unwrap_or(f64::NAN),
            low: low_ca.get(i).unwrap_or(f64::NAN),
            close: close_ca.get(i).unwrap_or(f64::NAN),
            volume: vol_ca.get(i).unwrap_or(0),
            adj_close: adj_ca.get(i).unwrap_or(f64::NAN),
        });
    }

    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_cache_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("trendlab_test_{}_{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
    fn write_and_load_roundtrip() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        cache.write("SPY", &sample_bars()).unwrap();
        let loaded = cache.load("SPY").unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].date, NaiveDate::from_ymd_opt(2024, 1, 2).unwrap());
        assert_eq!(loaded[0].open, 100.0);
        assert_eq!(loaded[1].close, 102.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        let result = cache.load("NONEXISTENT");
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_meta_roundtrip() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        cache.write("SPY", &sample_bars()).unwrap();
        let meta = cache.get_meta("SPY").unwrap();

        assert_eq!(meta.symbol, "SPY");
        assert_eq!(meta.bar_count, 2);
        assert_eq!(
            meta.start_date,
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_status_query() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        cache.write("SPY", &sample_bars()).unwrap();
        let statuses = cache.status(&["SPY", "QQQ"]);

        assert_eq!(statuses.len(), 2);
        assert!(statuses[0].cached);
        assert!(!statuses[1].cached);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn coverage_check() {
        let dir = temp_cache_dir();
        let cache = ParquetCache::new(&dir);

        cache.write("SPY", &sample_bars()).unwrap();

        assert_eq!(
            cache.covers_range(
                "SPY",
                NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                NaiveDate::from_ymd_opt(2024, 1, 3).unwrap()
            ),
            CoverageResult::FullyCovered
        );
        assert_eq!(
            cache.covers_range("QQQ", NaiveDate::default(), NaiveDate::default()),
            CoverageResult::NotCached
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
