//! Data provider trait and structured error types.
//!
//! The DataProvider trait abstracts over data sources (Yahoo Finance, CSV import,
//! Parquet import) so we can swap implementations and mock for tests.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Raw daily OHLCV bar from a data provider (before validation/adjustment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
    pub adj_close: f64,
}

/// Structured error types for data operations.
///
/// These are designed to be displayable in both CLI and TUI contexts.
#[derive(Debug, Error)]
pub enum DataError {
    #[error("network unreachable: {0}")]
    NetworkUnreachable(String),

    #[error("rate limited by provider (retry after {retry_after_secs}s)")]
    RateLimited { retry_after_secs: u64 },

    #[error("response format changed: {0}")]
    ResponseFormatChanged(String),

    #[error("authentication required: {0}")]
    AuthenticationRequired(String),

    #[error("symbol not found: {symbol}")]
    SymbolNotFound { symbol: String },

    #[error("hard stop: data provider has blocked requests (circuit breaker tripped)")]
    CircuitBreakerTripped,

    #[error("cache error: {0}")]
    CacheError(String),

    #[error("validation error: {0}")]
    ValidationError(String),

    #[error("parquet I/O error: {0}")]
    ParquetError(String),

    #[error("no cached data for symbol '{symbol}' — run `download {symbol}` first")]
    NoCachedData { symbol: String },

    #[error("data error: {0}")]
    Other(String),
}

/// Result of a successful data fetch for a single symbol.
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub symbol: String,
    pub bars: Vec<RawBar>,
    pub source: DataSource,
}

/// Where the data came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataSource {
    YahooFinance,
    CsvImport,
    ParquetImport,
    Cache,
    Synthetic,
}

/// Trait for data providers (Yahoo Finance, CSV import, etc).
///
/// Implementations handle the specifics of fetching data from a particular source.
/// The cache layer sits above this trait — providers don't know about the cache.
pub trait DataProvider: Send + Sync {
    /// Human-readable name of this provider.
    fn name(&self) -> &str;

    /// Fetch daily OHLCV bars for a symbol over a date range.
    fn fetch(
        &self,
        symbol: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<FetchResult, DataError>;

    /// Check if the provider is currently available (not rate-limited, not blocked).
    fn is_available(&self) -> bool;
}

/// Progress callback for multi-symbol operations.
pub trait DownloadProgress: Send {
    /// Called when starting to fetch a symbol.
    fn on_start(&self, symbol: &str, index: usize, total: usize);

    /// Called when a symbol fetch completes.
    fn on_complete(&self, symbol: &str, index: usize, total: usize, result: &Result<(), DataError>);

    /// Called when the entire batch is done.
    fn on_batch_complete(&self, succeeded: usize, failed: usize, total: usize);
}

/// Simple progress reporter that prints to stdout.
pub struct StdoutProgress;

impl DownloadProgress for StdoutProgress {
    fn on_start(&self, symbol: &str, index: usize, total: usize) {
        println!("[{}/{}] Fetching {symbol}...", index + 1, total);
    }

    fn on_complete(
        &self,
        symbol: &str,
        _index: usize,
        _total: usize,
        result: &Result<(), DataError>,
    ) {
        match result {
            Ok(()) => println!("  OK: {symbol}"),
            Err(e) => println!("  FAIL: {symbol}: {e}"),
        }
    }

    fn on_batch_complete(&self, succeeded: usize, failed: usize, total: usize) {
        println!("\nDownload complete: {succeeded}/{total} succeeded, {failed} failed");
    }
}
