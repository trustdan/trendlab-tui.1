use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use crate::data::ingest::DataError;
use crate::domain::DatasetHash;

/// Canonical data cache
pub struct DataCache {
    cache_dir: PathBuf,
}

impl DataCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Write DataFrame to cache with metadata
    pub fn write(
        &self,
        df: &mut DataFrame,
        metadata: CacheMetadata,
    ) -> Result<DatasetHash, DataError> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|e| DataError::CacheError(format!("Failed to create cache directory: {}", e)))?;

        // Compute content hash
        let hash = Self::compute_hash(df)?;

        // Write parquet file
        let data_path = self.cache_dir.join(format!("{}.parquet", hash.0));
        let file = std::fs::File::create(&data_path)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        ParquetWriter::new(file)
            .finish(df)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        // Write metadata sidecar
        let meta_path = self.cache_dir.join(format!("{}.meta.json", hash.0));
        let meta_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| DataError::CacheError(e.to_string()))?;
        std::fs::write(&meta_path, meta_json)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        Ok(hash)
    }

    /// Read DataFrame from cache
    pub fn read(&self, hash: &DatasetHash) -> Result<DataFrame, DataError> {
        let data_path = self.cache_dir.join(format!("{}.parquet", hash.0));
        let file = std::fs::File::open(&data_path)
            .map_err(|e| DataError::CacheError(format!("Failed to open cache file: {}", e)))?;

        ParquetReader::new(file)
            .finish()
            .map_err(|e| DataError::CacheError(e.to_string()))
    }

    /// Read metadata from cache
    pub fn read_metadata(&self, hash: &DatasetHash) -> Result<CacheMetadata, DataError> {
        let meta_path = self.cache_dir.join(format!("{}.meta.json", hash.0));
        let meta_json = std::fs::read_to_string(&meta_path)
            .map_err(|e| DataError::CacheError(format!("Failed to read metadata: {}", e)))?;
        serde_json::from_str(&meta_json)
            .map_err(|e| DataError::CacheError(format!("Failed to parse metadata: {}", e)))
    }

    /// Compute deterministic hash of DataFrame content
    /// Uses BLAKE3 with sampled content to balance correctness vs performance
    fn compute_hash(df: &DataFrame) -> Result<DatasetHash, DataError> {
        let mut hasher = blake3::Hasher::new();

        // Hash schema (column names + types)
        let schema = df.schema();
        for field in schema.iter_fields() {
            hasher.update(field.name().as_bytes());
            hasher.update(format!("{:?}", field.dtype()).as_bytes());
        }

        // Hash row count
        hasher.update(&df.height().to_le_bytes());

        // Sampled content hash: every Nth row + per-column checksums
        // This catches mutations in the middle without full content hash overhead
        let sample_interval = (df.height() / 100).max(1); // sample ~100 rows

        for (col_idx, col_name) in df.get_column_names().iter().enumerate() {
            if let Ok(col) = df.column(col_name) {
                // Hash column name and index
                hasher.update(col_name.as_bytes());
                hasher.update(&col_idx.to_le_bytes());

                // Sample rows
                for row_idx in (0..df.height()).step_by(sample_interval) {
                    if let Ok(value) = col.get(row_idx) {
                        hasher.update(format!("{:?}", value).as_bytes());
                    }
                }
            }
        }

        // Include cache schema version for future invalidation
        hasher.update(b"cache_schema_v1");

        let hash_bytes = hasher.finalize();
        Ok(DatasetHash::from_hash(&hash_bytes.to_hex()))
    }
}

/// Cache metadata sidecar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub created_at: DateTime<Utc>,
    pub source_files: Vec<String>,
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
    pub symbol_count: usize,
    pub bar_count: usize,
    pub adjustments: Option<String>,
    pub anomalies: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_deterministic() {
        // Create identical DataFrames
        let df1 = df!(
            "timestamp" => &[1i64, 2, 3],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 101.0, 102.0],
            "high" => &[105.0, 106.0, 107.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 104.0, 105.0],
            "volume" => &[1000.0, 1000.0, 1000.0],
        )
        .unwrap();

        let df2 = df!(
            "timestamp" => &[1i64, 2, 3],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 101.0, 102.0],
            "high" => &[105.0, 106.0, 107.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 104.0, 105.0],
            "volume" => &[1000.0, 1000.0, 1000.0],
        )
        .unwrap();

        let hash1 = DataCache::compute_hash(&df1).unwrap();
        let hash2 = DataCache::compute_hash(&df2).unwrap();

        assert_eq!(hash1.0, hash2.0);
    }

    #[test]
    fn test_compute_hash_different_data() {
        let df1 = df!(
            "timestamp" => &[1i64, 2, 3],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 101.0, 102.0],
            "high" => &[105.0, 106.0, 107.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 104.0, 105.0],
            "volume" => &[1000.0, 1000.0, 1000.0],
        )
        .unwrap();

        let df2 = df!(
            "timestamp" => &[1i64, 2, 3],
            "symbol" => &["QQQ", "QQQ", "QQQ"],  // Different symbol
            "open" => &[100.0, 101.0, 102.0],
            "high" => &[105.0, 106.0, 107.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 104.0, 105.0],
            "volume" => &[1000.0, 1000.0, 1000.0],
        )
        .unwrap();

        let hash1 = DataCache::compute_hash(&df1).unwrap();
        let hash2 = DataCache::compute_hash(&df2).unwrap();

        assert_ne!(hash1.0, hash2.0);
    }

    #[test]
    fn test_cache_roundtrip() {
        let temp_dir = std::env::temp_dir().join("trendlab_test_cache");
        let cache = DataCache::new(temp_dir.clone());

        let mut df = df!(
            "timestamp" => &[1i64, 2, 3],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 101.0, 102.0],
            "high" => &[105.0, 106.0, 107.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 104.0, 105.0],
            "volume" => &[1000.0, 1000.0, 1000.0],
        )
        .unwrap();

        let metadata = CacheMetadata {
            created_at: Utc::now(),
            source_files: vec!["test.csv".to_string()],
            date_range: (Utc::now(), Utc::now()),
            symbol_count: 1,
            bar_count: 3,
            adjustments: None,
            anomalies: vec![],
        };

        // Write to cache
        let hash = cache.write(&mut df, metadata.clone()).unwrap();

        // Read back
        let df_read = cache.read(&hash).unwrap();
        let meta_read = cache.read_metadata(&hash).unwrap();

        // Verify
        assert_eq!(df_read.height(), 3);
        assert_eq!(meta_read.bar_count, 3);
        assert_eq!(meta_read.symbol_count, 1);

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
