use chrono::{NaiveDate, Utc};
use std::collections::HashMap;
use trendlab_runner::reporting::{ArtifactManager};
use trendlab_runner::reporting::export::export_run_with_report;
use trendlab_runner::result::{BacktestResult, EquityPoint, PerformanceStats, ResultMetadata, TradeDirection, TradeRecord};

fn make_result(run_id: &str) -> BacktestResult {
    BacktestResult {
        run_id: run_id.to_string(),
        equity_curve: vec![
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                equity: 100_000.0,
            },
            EquityPoint {
                date: NaiveDate::from_ymd_opt(2023, 1, 2).unwrap(),
                equity: 101_000.0,
            },
        ],
        trades: vec![TradeRecord {
            symbol: "SPY".to_string(),
            entry_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
            exit_date: NaiveDate::from_ymd_opt(2023, 1, 2).unwrap(),
            direction: TradeDirection::Long,
            entry_price: 100.0,
            exit_price: 101.0,
            quantity: 100,
            pnl: 100.0,
            return_pct: 1.0,
            signal_intent: Some("Long".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $100.00".to_string()),
            entry_slippage: Some(0.05),
            exit_slippage: Some(0.03),
            entry_was_gapped: Some(false),
            exit_was_gapped: Some(false),
        }],
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
fn test_artifact_manager_exports() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = ArtifactManager::new(temp_dir.path()).unwrap();
    let result = make_result("report_test_run");

    let paths = manager.save_run(&result).unwrap();
    assert!(paths.manifest.exists());
    assert!(paths.equity_csv.exists());
    assert!(paths.equity_parquet.exists());
    assert!(paths.trades_csv.exists());
    assert!(paths.trades_json.exists());
    assert!(paths.diagnostics_json.exists());
}

#[test]
fn test_export_with_report() {
    let temp_dir = tempfile::tempdir().unwrap();
    let result = make_result("report_test_run_report");

    let paths = export_run_with_report(temp_dir.path(), &result, true).unwrap();
    assert!(paths.report_markdown.is_some());
    assert!(paths.report_markdown.unwrap().exists());
}
