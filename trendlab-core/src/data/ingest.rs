//! Ingest pipeline — validation, canonicalization, corporate action adjustment.
//!
//! Raw data from any source (Yahoo, CSV, Parquet import) passes through this
//! pipeline before being cached:
//! 1. Schema validation (OHLCV sanity)
//! 2. Sort by date ascending
//! 3. Deduplicate (keep last row per date)
//! 4. Corporate action adjustment (split-adjust all OHLC columns)
//! 5. Anomaly detection

use super::provider::{DataError, RawBar};

/// Result of the ingest pipeline.
#[derive(Debug)]
pub struct IngestResult {
    /// Validated, sorted, deduplicated, adjusted bars.
    pub bars: Vec<RawBar>,
    /// Number of bars removed as duplicates.
    pub duplicates_removed: usize,
    /// Number of bars with OHLCV anomalies (but not removed).
    pub anomalies_detected: usize,
    /// Per-bar adjustment ratios (adj_close / close).
    pub adjustment_ratios: Vec<f64>,
}

/// Run the full ingest pipeline on raw bars.
pub fn ingest(mut bars: Vec<RawBar>) -> Result<IngestResult, DataError> {
    if bars.is_empty() {
        return Err(DataError::ValidationError("no bars to ingest".into()));
    }

    // Step 1: Sort by date ascending
    bars.sort_by_key(|b| b.date);

    // Step 2: Deduplicate (keep last row per date)
    let len_before = bars.len();
    bars.dedup_by_key(|b| b.date);
    let duplicates_removed = len_before - bars.len();

    // Step 3: Validate OHLCV sanity
    let mut anomalies_detected = 0;
    for bar in &bars {
        if !validate_bar(bar) {
            anomalies_detected += 1;
        }
    }

    // Step 4: Corporate action adjustment
    let adjustment_ratios = adjust_corporate_actions(&mut bars);

    Ok(IngestResult {
        bars,
        duplicates_removed,
        anomalies_detected,
        adjustment_ratios,
    })
}

/// Validate a single bar for OHLCV sanity.
fn validate_bar(bar: &RawBar) -> bool {
    // NaN bars pass validation (they represent void bars, handled by the engine)
    if bar.open.is_nan() || bar.close.is_nan() {
        return true;
    }
    bar.high >= bar.low
        && bar.high >= bar.open
        && bar.high >= bar.close
        && bar.low <= bar.open
        && bar.low <= bar.close
        && bar.open > 0.0
        && bar.close > 0.0
}

/// Apply corporate action adjustment to all OHLC columns.
///
/// The adjustment ratio is `adj_close / close` for each bar. All four OHLC columns
/// are multiplied by this ratio. Volume is reverse-adjusted (divided by the ratio)
/// to preserve notional volume.
///
/// This prevents the "ATR on raw OHLC with adjusted close" bug where volatility
/// measures mix adjusted and unadjusted price scales across split boundaries.
fn adjust_corporate_actions(bars: &mut [RawBar]) -> Vec<f64> {
    let mut ratios = Vec::with_capacity(bars.len());

    for bar in bars.iter_mut() {
        if bar.close.is_nan() || bar.adj_close.is_nan() || bar.close == 0.0 {
            ratios.push(1.0);
            continue;
        }

        let ratio = bar.adj_close / bar.close;
        ratios.push(ratio);

        // Only adjust if the ratio is materially different from 1.0
        if (ratio - 1.0).abs() > 1e-8 {
            bar.open *= ratio;
            bar.high *= ratio;
            bar.low *= ratio;
            bar.close *= ratio;
            // Reverse-adjust volume to preserve notional volume
            if ratio > 0.0 {
                bar.volume = (bar.volume as f64 / ratio).round() as u64;
            }
        }
    }

    ratios
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_bar(date: &str, ohlc: (f64, f64, f64, f64), volume: u64, adj: f64) -> RawBar {
        RawBar {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            open: ohlc.0,
            high: ohlc.1,
            low: ohlc.2,
            close: ohlc.3,
            volume,
            adj_close: adj,
        }
    }

    #[test]
    fn ingest_sorts_by_date() {
        let bars = vec![
            make_bar("2024-01-03", (102.0, 104.0, 101.0, 103.0), 1000, 103.0),
            make_bar("2024-01-02", (100.0, 102.0, 99.0, 101.0), 1000, 101.0),
        ];
        let result = ingest(bars).unwrap();
        assert_eq!(
            result.bars[0].date,
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
        );
    }

    #[test]
    fn ingest_deduplicates() {
        let bars = vec![
            make_bar("2024-01-02", (100.0, 102.0, 99.0, 101.0), 1000, 101.0),
            make_bar("2024-01-02", (100.5, 102.5, 99.5, 101.5), 1100, 101.5),
            make_bar("2024-01-03", (102.0, 104.0, 101.0, 103.0), 1000, 103.0),
        ];
        let result = ingest(bars).unwrap();
        assert_eq!(result.bars.len(), 2);
        assert_eq!(result.duplicates_removed, 1);
    }

    #[test]
    fn corporate_action_adjustment() {
        // Simulate a 2:1 stock split: adj_close is half of close
        let bars = vec![make_bar(
            "2024-01-02",
            (200.0, 204.0, 198.0, 202.0),
            500,
            101.0,
        )];
        let result = ingest(bars).unwrap();
        let bar = &result.bars[0];
        // ratio = 101.0 / 202.0 = 0.5
        assert!((bar.open - 100.0).abs() < 0.01);
        assert!((bar.high - 102.0).abs() < 0.01);
        assert!((bar.low - 99.0).abs() < 0.01);
        assert!((bar.close - 101.0).abs() < 0.01);
        // Volume reverse-adjusted: 500 / 0.5 = 1000
        assert_eq!(bar.volume, 1000);
    }

    #[test]
    fn no_adjustment_when_ratio_is_one() {
        let bars = vec![make_bar(
            "2024-01-02",
            (100.0, 102.0, 99.0, 101.0),
            1000,
            101.0,
        )];
        let result = ingest(bars).unwrap();
        let bar = &result.bars[0];
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.volume, 1000);
    }

    #[test]
    fn empty_bars_fails() {
        assert!(ingest(vec![]).is_err());
    }

    #[test]
    fn anomaly_detection_flags_bad_bars() {
        let bars = vec![
            make_bar("2024-01-02", (100.0, 98.0, 102.0, 101.0), 1000, 101.0), // high < low
        ];
        let result = ingest(bars).unwrap();
        assert_eq!(result.anomalies_detected, 1);
    }
}
