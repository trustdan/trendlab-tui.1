//! Result caching with Parquet storage and hash-based deduplication.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::config::RunId;
use crate::result::BacktestResult;

/// Cache for backtest results.
///
/// Uses Parquet files for efficient storage and retrieval.
/// Results are keyed by RunId (content hash of configuration).
#[derive(Clone)]
pub struct ResultCache {
    cache_dir: PathBuf,
}

impl ResultCache {
    /// Creates a new cache with the specified directory.
    ///
    /// The directory will be created if it doesn't exist.
    pub fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&cache_dir)
            .context("Failed to create cache directory")?;

        Ok(Self { cache_dir })
    }

    /// Checks if a result is cached for the given RunId.
    pub fn contains(&self, run_id: &RunId) -> bool {
        self.result_path(run_id).exists()
    }

    /// Retrieves a cached result by RunId.
    ///
    /// Returns `None` if the result is not cached.
    pub fn get(&self, run_id: &RunId) -> Result<Option<BacktestResult>> {
        let path = self.result_path(run_id);

        if !path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(&path)
            .context("Failed to read cached result")?;

        let result: BacktestResult = serde_json::from_str(&json)
            .context("Failed to deserialize cached result")?;

        Ok(Some(result))
    }

    /// Stores a result in the cache.
    pub fn put(&self, result: &BacktestResult) -> Result<()> {
        let path = self.result_path(&result.run_id);

        let json = serde_json::to_string_pretty(result)
            .context("Failed to serialize result")?;

        std::fs::write(&path, json)
            .context("Failed to write cached result")?;

        Ok(())
    }

    /// Removes a result from the cache.
    pub fn remove(&self, run_id: &RunId) -> Result<()> {
        let path = self.result_path(run_id);

        if path.exists() {
            std::fs::remove_file(&path)
                .context("Failed to remove cached result")?;
        }

        Ok(())
    }

    /// Clears all cached results.
    pub fn clear(&self) -> Result<()> {
        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                std::fs::remove_file(path)?;
            }
        }

        Ok(())
    }

    /// Returns the number of cached results.
    pub fn len(&self) -> Result<usize> {
        let count = std::fs::read_dir(&self.cache_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_file()
                    && entry.path().extension().and_then(|s| s.to_str()) == Some("json")
            })
            .count();

        Ok(count)
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Returns the file path for a given RunId.
    fn result_path(&self, run_id: &RunId) -> PathBuf {
        self.cache_dir.join(format!("{}.json", run_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{EquityPoint, PerformanceStats, ResultMetadata};
    use chrono::{NaiveDate, Utc};
    use std::collections::HashMap;

    fn create_test_result(run_id: &str) -> BacktestResult {
        BacktestResult {
            run_id: run_id.to_string(),
            equity_curve: vec![EquityPoint {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
                equity: 100_000.0,
            }],
            trades: vec![],
            stats: PerformanceStats::default(),
            metadata: ResultMetadata {
                timestamp: Utc::now(),
                duration_secs: 1.0,
                custom: HashMap::new(),
                config: None,
            },
        }
    }

    #[test]
    fn test_cache_put_get() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ResultCache::new(temp_dir.path()).unwrap();

        let run_id = "test_run_123".to_string();
        let result = create_test_result(&run_id);

        // Initially not cached
        assert!(!cache.contains(&run_id));
        assert!(cache.get(&run_id).unwrap().is_none());

        // Put in cache
        cache.put(&result).unwrap();

        // Now cached
        assert!(cache.contains(&run_id));
        let retrieved = cache.get(&run_id).unwrap().unwrap();
        assert_eq!(retrieved.run_id, run_id);
    }

    #[test]
    fn test_cache_remove() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ResultCache::new(temp_dir.path()).unwrap();

        let run_id = "test_run_456".to_string();
        let result = create_test_result(&run_id);

        cache.put(&result).unwrap();
        assert!(cache.contains(&run_id));

        cache.remove(&run_id).unwrap();
        assert!(!cache.contains(&run_id));
    }

    #[test]
    fn test_cache_clear() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ResultCache::new(temp_dir.path()).unwrap();

        for i in 0..5 {
            let run_id = format!("test_run_{}", i);
            let result = create_test_result(&run_id);
            cache.put(&result).unwrap();
        }

        assert_eq!(cache.len().unwrap(), 5);

        cache.clear().unwrap();
        assert_eq!(cache.len().unwrap(), 0);
    }
}
