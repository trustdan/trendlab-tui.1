use polars::prelude::*;

/// Canonicalizer for bar data
pub struct Canonicalizer;

impl Canonicalizer {
    /// Canonicalize data: sort, dedupe, validate
    pub fn canonicalize(df: LazyFrame) -> LazyFrame {
        df.sort(
            ["timestamp", "symbol"],
            SortMultipleOptions::default()
                .with_order_descending_multi([false, false])
                .with_maintain_order(true),
        )
        .unique_stable(
            Some(vec!["timestamp".into(), "symbol".into()]),
            UniqueKeepStrategy::First,
        )
    }

    /// Validate bar data (no negative prices, high >= low, etc.)
    pub fn validate(df: LazyFrame) -> LazyFrame {
        df.filter(
            col("high")
                .gt_eq(col("low"))
                .and(col("open").gt(0.0))
                .and(col("high").gt(0.0))
                .and(col("low").gt(0.0))
                .and(col("close").gt(0.0))
                .and(col("volume").gt_eq(0.0))
                .and(col("open").gt_eq(col("low")))
                .and(col("open").lt_eq(col("high")))
                .and(col("close").gt_eq(col("low")))
                .and(col("close").lt_eq(col("high"))),
        )
    }

    /// Align multi-symbol timestamps to canonical index
    /// Prevents "shift bugs" where symbols fall out of sync due to data gaps
    pub fn align_multi_symbol_timestamps(df: LazyFrame) -> LazyFrame {
        // 1. Extract unique timestamps across ALL symbols
        // 2. For each symbol, reindex to the canonical timestamp set
        // 3. Apply forward-fill (or explicit null policy) for missing bars
        // This ensures bar[i] for all symbols shares the same timestamp

        // Implementation note: Use Polars join_asof or pivot operations
        // to ensure every symbol has a row for every timestamp in the universe
        df
        // TODO: Implement canonical timestamp alignment
        // Example strategy:
        // - Get all unique timestamps
        // - Pivot to wide format (symbol columns)
        // - Forward-fill nulls (or reject if too many gaps)
        // - Unpivot back to long format
    }

    /// Detect anomalies (outliers, gaps, suspicious volume)
    pub fn detect_anomalies(df: &DataFrame) -> Vec<AnomalyReport> {
        let mut anomalies = Vec::new();

        // Check for zero volume
        if let Ok(volume) = df.column("volume") {
            let zero_volume_count = volume
                .f64()
                .unwrap()
                .iter()
                .filter(|v| v == &Some(0.0))
                .count();

            if zero_volume_count > 0 {
                anomalies.push(AnomalyReport {
                    anomaly_type: AnomalyType::ZeroVolume,
                    count: zero_volume_count,
                    severity: Severity::Warning,
                });
            }
        }

        // More anomaly checks would go here...

        anomalies
    }
}

#[derive(Debug)]
pub struct AnomalyReport {
    pub anomaly_type: AnomalyType,
    pub count: usize,
    pub severity: Severity,
}

#[derive(Debug, PartialEq)]
pub enum AnomalyType {
    ZeroVolume,
    SuspiciousGap,
    OutlierPrice,
}

#[derive(Debug, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalize_sorts_data() {
        // Create test DataFrame with unsorted data
        let df = df!(
            "timestamp" => &[3i64, 1, 2],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 100.0, 100.0],
            "high" => &[105.0, 105.0, 105.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 103.0, 103.0],
            "volume" => &[1000.0, 1000.0, 1000.0],
        )
        .unwrap();

        let sorted = Canonicalizer::canonicalize(df.lazy()).collect().unwrap();
        let timestamps = sorted.column("timestamp").unwrap().i64().unwrap();

        eprintln!("Sorted timestamps: {:?}", timestamps);
        assert_eq!(timestamps.get(0), Some(1));
        assert_eq!(timestamps.get(1), Some(2));
        assert_eq!(timestamps.get(2), Some(3));
    }

    #[test]
    fn test_canonicalize_removes_duplicates() {
        // Create test DataFrame with duplicate timestamp+symbol
        let df = df!(
            "timestamp" => &[1i64, 1, 2],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 101.0, 102.0],
            "high" => &[105.0, 106.0, 107.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 104.0, 105.0],
            "volume" => &[1000.0, 2000.0, 3000.0],
        )
        .unwrap();

        let deduped = Canonicalizer::canonicalize(df.lazy()).collect().unwrap();

        assert_eq!(deduped.height(), 2);
        // First occurrence should be kept
        let opens = deduped.column("open").unwrap().f64().unwrap();
        assert_eq!(opens.get(0), Some(100.0));
    }

    #[test]
    fn test_validate_rejects_inverted_bars() {
        // Create test DataFrame with high < low
        let df = df!(
            "timestamp" => &[1i64, 2],
            "symbol" => &["SPY", "SPY"],
            "open" => &[100.0, 100.0],
            "high" => &[95.0, 105.0],  // First bar has high < low
            "low" => &[105.0, 99.0],
            "close" => &[102.0, 103.0],
            "volume" => &[1000.0, 1000.0],
        )
        .unwrap();

        let validated = Canonicalizer::validate(df.lazy()).collect().unwrap();

        // Only valid bar should remain
        assert_eq!(validated.height(), 1);
        let timestamps = validated.column("timestamp").unwrap().i64().unwrap();
        assert_eq!(timestamps.get(0), Some(2));
    }

    #[test]
    fn test_validate_rejects_negative_prices() {
        let df = df!(
            "timestamp" => &[1i64, 2],
            "symbol" => &["SPY", "SPY"],
            "open" => &[-100.0, 100.0],  // Negative price
            "high" => &[105.0, 105.0],
            "low" => &[99.0, 99.0],
            "close" => &[103.0, 103.0],
            "volume" => &[1000.0, 1000.0],
        )
        .unwrap();

        let validated = Canonicalizer::validate(df.lazy()).collect().unwrap();

        assert_eq!(validated.height(), 1);
    }

    #[test]
    fn test_detect_anomalies_flags_zero_volume() {
        let df = df!(
            "timestamp" => &[1i64, 2, 3],
            "symbol" => &["SPY", "SPY", "SPY"],
            "open" => &[100.0, 100.0, 100.0],
            "high" => &[105.0, 105.0, 105.0],
            "low" => &[99.0, 99.0, 99.0],
            "close" => &[103.0, 103.0, 103.0],
            "volume" => &[0.0, 1000.0, 0.0],
        )
        .unwrap();

        let anomalies = Canonicalizer::detect_anomalies(&df);

        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].anomaly_type, AnomalyType::ZeroVolume);
        assert_eq!(anomalies[0].count, 2);
        assert_eq!(anomalies[0].severity, Severity::Warning);
    }
}
