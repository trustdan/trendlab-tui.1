//! Download orchestrator — coordinates multi-symbol downloads with progress reporting.

use super::cache::ParquetCache;
use super::ingest;
use super::provider::{DataError, DataProvider, DownloadProgress};
use chrono::NaiveDate;

/// Download multiple symbols, running them through the ingest pipeline and caching.
///
/// Returns a summary of successes and failures.
pub fn download_symbols(
    provider: &dyn DataProvider,
    cache: &ParquetCache,
    symbols: &[&str],
    start: NaiveDate,
    end: NaiveDate,
    force: bool,
    progress: &dyn DownloadProgress,
) -> DownloadSummary {
    let total = symbols.len();
    let mut succeeded = 0;
    let mut failed = 0;
    let mut errors: Vec<(String, DataError)> = Vec::new();

    for (i, symbol) in symbols.iter().enumerate() {
        progress.on_start(symbol, i, total);

        // Skip if cache is fresh and not forcing
        if !force {
            if let super::cache::CoverageResult::FullyCovered =
                cache.covers_range(symbol, start, end)
            {
                progress.on_complete(symbol, i, total, &Ok(()));
                succeeded += 1;
                continue;
            }
        }

        let result = download_single(provider, cache, symbol, start, end);
        progress.on_complete(symbol, i, total, &result);

        match result {
            Ok(()) => succeeded += 1,
            Err(e) => {
                errors.push((symbol.to_string(), e));
                failed += 1;
            }
        }

        // Bail out early if circuit breaker tripped
        if !provider.is_available() {
            for sym in &symbols[(i + 1)..total] {
                errors.push((sym.to_string(), DataError::CircuitBreakerTripped));
                failed += 1;
            }
            break;
        }
    }

    progress.on_batch_complete(succeeded, failed, total);

    DownloadSummary {
        total,
        succeeded,
        failed,
        errors,
    }
}

/// Download a single symbol: fetch → ingest → cache.
fn download_single(
    provider: &dyn DataProvider,
    cache: &ParquetCache,
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<(), DataError> {
    let fetch_result = provider.fetch(symbol, start, end)?;
    let ingest_result = ingest::ingest(fetch_result.bars)?;
    cache.write(symbol, &ingest_result.bars)?;
    Ok(())
}

/// Summary of a batch download operation.
#[derive(Debug)]
pub struct DownloadSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub errors: Vec<(String, DataError)>,
}

impl DownloadSummary {
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }
}
