//! Data pipeline — download, validate, cache, and serve market data.
//!
//! This module implements Track A of the build plan:
//! - Data provider abstraction (Yahoo Finance, CSV import)
//! - Ingest pipeline (validation, corporate action adjustment)
//! - Parquet cache with Hive-style partitioning
//! - Multi-symbol time alignment
//! - Universe configuration (sector/ticker hierarchy)
//! - Download orchestration with progress reporting

pub mod align;
pub mod cache;
pub mod circuit_breaker;
pub mod download;
pub mod ingest;
pub mod provider;
pub mod universe;
pub mod yahoo;

pub use cache::{CacheStatus, CoverageResult, ParquetCache};
pub use circuit_breaker::CircuitBreaker;
pub use download::{download_symbols, DownloadSummary};
pub use provider::{
    DataError, DataProvider, DataSource, DownloadProgress, FetchResult, RawBar, StdoutProgress,
};
pub use universe::Universe;
pub use yahoo::YahooProvider;
