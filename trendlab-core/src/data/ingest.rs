use polars::prelude::*;
use std::path::Path;
use std::sync::Arc;
use crate::data::schema::BarSchema;

/// Data ingestor for CSV and Parquet files
pub struct DataIngestor {
    schema: Schema,
}

impl DataIngestor {
    pub fn new() -> Self {
        Self {
            schema: BarSchema::schema(),
        }
    }

    /// Ingest CSV file
    pub fn ingest_csv(&self, path: &Path) -> Result<LazyFrame, DataError> {
        LazyCsvReader::new(path)
            .with_schema(Some(Arc::new(self.schema.clone())))
            .with_has_header(true)
            .finish()
            .map_err(|e| DataError::IngestFailed(e.to_string()))
    }

    /// Ingest Parquet file
    pub fn ingest_parquet(&self, path: &Path) -> Result<LazyFrame, DataError> {
        LazyFrame::scan_parquet(path, Default::default())
            .map_err(|e| DataError::IngestFailed(e.to_string()))
    }
}

impl Default for DataIngestor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("Ingest failed: {0}")]
    IngestFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Cache error: {0}")]
    CacheError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingestor_creation() {
        let ingestor = DataIngestor::new();
        assert!(ingestor.schema.contains("timestamp"));
        assert!(ingestor.schema.contains("symbol"));
        assert!(ingestor.schema.contains("open"));
    }

    // Note: Full ingestion tests require actual CSV files,
    // which will be added when creating test fixtures
}
