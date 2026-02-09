//! Reporting and export — JSON, CSV, and Markdown artifact generation.
//!
//! Provides three export formats for backtest results:
//! - **JSON**: full round-trip serialization with schema versioning
//! - **CSV**: trade tape and equity curve for external analysis tools
//! - **Markdown**: human-readable single-run reports and side-by-side comparisons
//!
//! All persisted artifacts include a `schema_version` field. Unknown versions
//! are rejected on load.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use trendlab_core::domain::TradeRecord;

use crate::runner::{BacktestResult, SCHEMA_VERSION};

// ─── JSON export ────────────────────────────────────────────────────

/// Serialize a `BacktestResult` to pretty JSON.
pub fn export_json(result: &BacktestResult) -> Result<String> {
    serde_json::to_string_pretty(result).context("failed to serialize BacktestResult to JSON")
}

/// Deserialize a `BacktestResult` from JSON, rejecting unknown schema versions.
pub fn import_json(json: &str) -> Result<BacktestResult> {
    let result: BacktestResult =
        serde_json::from_str(json).context("failed to deserialize BacktestResult from JSON")?;
    if result.schema_version > SCHEMA_VERSION {
        bail!(
            "unsupported schema version {} (max supported: {})",
            result.schema_version,
            SCHEMA_VERSION
        );
    }
    Ok(result)
}

// ─── CSV export ─────────────────────────────────────────────────────

/// Export a trade list as CSV with all columns including signal trace fields.
///
/// Columns: symbol, side, entry_bar, entry_date, entry_price, exit_bar,
/// exit_date, exit_price, quantity, gross_pnl, commission, slippage, net_pnl,
/// bars_held, mae, mfe, signal_type, pm_type, execution_model, filter_type
pub fn export_trades_csv(trades: &[TradeRecord]) -> Result<String> {
    let mut wtr = csv::Writer::from_writer(vec![]);

    // Header
    wtr.write_record([
        "symbol",
        "side",
        "entry_bar",
        "entry_date",
        "entry_price",
        "exit_bar",
        "exit_date",
        "exit_price",
        "quantity",
        "gross_pnl",
        "commission",
        "slippage",
        "net_pnl",
        "bars_held",
        "mae",
        "mfe",
        "signal_type",
        "pm_type",
        "execution_model",
        "filter_type",
    ])?;

    for t in trades {
        wtr.write_record([
            &t.symbol,
            &format!("{:?}", t.side),
            &t.entry_bar.to_string(),
            &t.entry_date.to_string(),
            &format!("{:.6}", t.entry_price),
            &t.exit_bar.to_string(),
            &t.exit_date.to_string(),
            &format!("{:.6}", t.exit_price),
            &format!("{:.6}", t.quantity),
            &format!("{:.2}", t.gross_pnl),
            &format!("{:.2}", t.commission),
            &format!("{:.2}", t.slippage),
            &format!("{:.2}", t.net_pnl),
            &t.bars_held.to_string(),
            &format!("{:.2}", t.mae),
            &format!("{:.2}", t.mfe),
            t.signal_type.as_deref().unwrap_or(""),
            t.pm_type.as_deref().unwrap_or(""),
            t.execution_model.as_deref().unwrap_or(""),
            t.filter_type.as_deref().unwrap_or(""),
        ])?;
    }

    let data = wtr.into_inner().context("failed to flush CSV writer")?;
    String::from_utf8(data).context("CSV output is not valid UTF-8")
}

/// Export an equity curve as CSV with bar_index and equity columns.
pub fn export_equity_csv(equity_curve: &[f64]) -> Result<String> {
    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(["bar_index", "equity"])?;
    for (i, eq) in equity_curve.iter().enumerate() {
        wtr.write_record([&i.to_string(), &format!("{:.2}", eq)])?;
    }
    let data = wtr.into_inner().context("failed to flush CSV writer")?;
    String::from_utf8(data).context("CSV output is not valid UTF-8")
}

// ─── Artifact bundle ────────────────────────────────────────────────

/// Save the full artifact set for a single backtest run.
///
/// Creates a directory named `{symbol}_{timestamp}/` under `output_dir`
/// containing:
/// - `manifest.json` — the full `BacktestResult`
/// - `trades.csv` — trade tape with signal trace columns
/// - `equity.csv` — bar-by-bar equity curve
///
/// Returns the path to the created directory.
pub fn save_artifacts(result: &BacktestResult, output_dir: &Path) -> Result<PathBuf> {
    let dirname = format!(
        "{}_{}",
        result.symbol,
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    let run_dir = output_dir.join(dirname);
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create artifact dir: {}", run_dir.display()))?;

    // manifest.json
    let json = export_json(result)?;
    std::fs::write(run_dir.join("manifest.json"), &json)?;

    // trades.csv
    let trades_csv = export_trades_csv(&result.trades)?;
    std::fs::write(run_dir.join("trades.csv"), &trades_csv)?;

    // equity.csv
    let equity_csv = export_equity_csv(&result.equity_curve)?;
    std::fs::write(run_dir.join("equity.csv"), &equity_csv)?;

    Ok(run_dir)
}

/// Load a `BacktestResult` from an artifact directory's manifest.json.
///
/// Rejects unknown schema versions.
pub fn load_artifacts(dir: &Path) -> Result<BacktestResult> {
    let manifest_path = dir.join("manifest.json");
    let json = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    import_json(&json)
}

// ─── Markdown reports ───────────────────────────────────────────────

/// Generate a Markdown report for a single backtest run.
pub fn generate_report(result: &BacktestResult) -> String {
    let mut md = String::with_capacity(2048);

    md.push_str("# Backtest Report\n\n");

    // Metadata
    md.push_str("## Metadata\n\n");
    md.push_str("| Field | Value |\n");
    md.push_str("| --- | --- |\n");
    md.push_str(&format!("| Symbol | {} |\n", result.symbol));
    md.push_str(&format!(
        "| Period | {} to {} |\n",
        result.start_date, result.end_date
    ));
    md.push_str(&format!(
        "| Initial Capital | ${:.0} |\n",
        result.initial_capital
    ));
    md.push_str(&format!(
        "| Bars | {} ({} warmup) |\n",
        result.bar_count, result.warmup_bars
    ));
    md.push_str(&format!("| Signals | {} |\n", result.signal_count));
    md.push_str(&format!("| Dataset Hash | {} |\n", result.dataset_hash));
    if result.has_synthetic {
        md.push_str("| Data | **SYNTHETIC** |\n");
    }
    md.push('\n');

    // Strategy Composition
    md.push_str("## Strategy Composition\n\n");
    md.push_str(&format_component("Signal", &result.config.signal));
    md.push_str(&format_component(
        "Position Manager",
        &result.config.position_manager,
    ));
    md.push_str(&format_component(
        "Execution Model",
        &result.config.execution_model,
    ));
    md.push_str(&format_component(
        "Signal Filter",
        &result.config.signal_filter,
    ));
    md.push('\n');

    // Performance Summary
    let m = &result.metrics;
    md.push_str("## Performance Summary\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("| --- | --- |\n");
    md.push_str(&format!(
        "| Total Return | {:.2}% |\n",
        m.total_return * 100.0
    ));
    md.push_str(&format!("| CAGR | {:.2}% |\n", m.cagr * 100.0));
    md.push_str(&format!("| Sharpe | {:.3} |\n", m.sharpe));
    md.push_str(&format!("| Sortino | {:.3} |\n", m.sortino));
    md.push_str(&format!("| Calmar | {:.3} |\n", m.calmar));
    md.push_str(&format!(
        "| Max Drawdown | {:.2}% |\n",
        m.max_drawdown * 100.0
    ));
    md.push_str(&format!("| Win Rate | {:.1}% |\n", m.win_rate * 100.0));
    md.push_str(&format!("| Profit Factor | {:.2} |\n", m.profit_factor));
    md.push_str(&format!("| Trades | {} |\n", m.trade_count));
    md.push_str(&format!("| Turnover | {:.1}x |\n", m.turnover));
    md.push_str(&format!(
        "| Max Consecutive Wins | {} |\n",
        m.max_consecutive_wins
    ));
    md.push_str(&format!(
        "| Max Consecutive Losses | {} |\n",
        m.max_consecutive_losses
    ));
    md.push_str(&format!(
        "| Avg Losing Streak | {:.1} |\n",
        m.avg_losing_streak
    ));
    md.push('\n');

    // Stickiness Diagnostics
    if let Some(ref s) = result.stickiness {
        md.push_str("## Stickiness Diagnostics\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("| --- | --- |\n");
        md.push_str(&format!(
            "| Median Holding (bars) | {:.1} |\n",
            s.median_holding_bars
        ));
        md.push_str(&format!(
            "| P95 Holding (bars) | {:.1} |\n",
            s.p95_holding_bars
        ));
        md.push_str(&format!(
            "| % Over 60 bars | {:.1}% |\n",
            s.pct_over_60_bars * 100.0
        ));
        md.push_str(&format!(
            "| % Over 120 bars | {:.1}% |\n",
            s.pct_over_120_bars * 100.0
        ));
        md.push_str(&format!(
            "| Exit Trigger Rate | {:.3} |\n",
            s.exit_trigger_rate
        ));
        md.push_str(&format!(
            "| Reference Chase Ratio | {:.1} |\n",
            s.reference_chase_ratio
        ));
        md.push('\n');
    }

    // Data Quality
    if !result.data_quality_warnings.is_empty() || !result.void_bar_rates.is_empty() {
        md.push_str("## Data Quality\n\n");
        for warn in &result.data_quality_warnings {
            md.push_str(&format!("- {warn}\n"));
        }
        if !result.void_bar_rates.is_empty() {
            md.push_str("\nVoid bar rates:\n\n");
            for (sym, rate) in &result.void_bar_rates {
                md.push_str(&format!("- {sym}: {:.2}%\n", rate * 100.0));
            }
        }
        md.push('\n');
    }

    md
}

/// Generate a Markdown comparison report for two backtest results.
pub fn generate_comparison(a: &BacktestResult, b: &BacktestResult) -> String {
    let mut md = String::with_capacity(2048);

    md.push_str("# Strategy Comparison\n\n");

    // Composition diff
    md.push_str("## Composition\n\n");
    md.push_str("| Component | Strategy A | Strategy B |\n");
    md.push_str("| --- | --- | --- |\n");
    md.push_str(&format!(
        "| Symbol | {} | {} |\n",
        a.symbol, b.symbol
    ));
    md.push_str(&format!(
        "| Signal | {} | {} |\n",
        a.config.signal.component_type, b.config.signal.component_type
    ));
    md.push_str(&format!(
        "| Position Manager | {} | {} |\n",
        a.config.position_manager.component_type, b.config.position_manager.component_type
    ));
    md.push_str(&format!(
        "| Execution Model | {} | {} |\n",
        a.config.execution_model.component_type, b.config.execution_model.component_type
    ));
    md.push_str(&format!(
        "| Signal Filter | {} | {} |\n",
        a.config.signal_filter.component_type, b.config.signal_filter.component_type
    ));
    md.push('\n');

    // Side-by-side metrics
    md.push_str("## Performance Comparison\n\n");
    md.push_str("| Metric | Strategy A | Strategy B | Delta |\n");
    md.push_str("| --- | ---: | ---: | ---: |\n");

    let ma = &a.metrics;
    let mb = &b.metrics;

    fn pct(v: f64) -> String {
        format!("{:.2}%", v * 100.0)
    }
    fn f3(v: f64) -> String {
        format!("{:.3}", v)
    }
    fn f2(v: f64) -> String {
        format!("{:.2}", v)
    }
    fn delta_pct(a: f64, b: f64) -> String {
        let d = (b - a) * 100.0;
        if d >= 0.0 {
            format!("+{:.2}%", d)
        } else {
            format!("{:.2}%", d)
        }
    }
    fn delta_f3(a: f64, b: f64) -> String {
        let d = b - a;
        if d >= 0.0 {
            format!("+{:.3}", d)
        } else {
            format!("{:.3}", d)
        }
    }

    md.push_str(&format!(
        "| Total Return | {} | {} | {} |\n",
        pct(ma.total_return),
        pct(mb.total_return),
        delta_pct(ma.total_return, mb.total_return)
    ));
    md.push_str(&format!(
        "| CAGR | {} | {} | {} |\n",
        pct(ma.cagr),
        pct(mb.cagr),
        delta_pct(ma.cagr, mb.cagr)
    ));
    md.push_str(&format!(
        "| Sharpe | {} | {} | {} |\n",
        f3(ma.sharpe),
        f3(mb.sharpe),
        delta_f3(ma.sharpe, mb.sharpe)
    ));
    md.push_str(&format!(
        "| Sortino | {} | {} | {} |\n",
        f3(ma.sortino),
        f3(mb.sortino),
        delta_f3(ma.sortino, mb.sortino)
    ));
    md.push_str(&format!(
        "| Calmar | {} | {} | {} |\n",
        f3(ma.calmar),
        f3(mb.calmar),
        delta_f3(ma.calmar, mb.calmar)
    ));
    md.push_str(&format!(
        "| Max Drawdown | {} | {} | {} |\n",
        pct(ma.max_drawdown),
        pct(mb.max_drawdown),
        delta_pct(ma.max_drawdown, mb.max_drawdown)
    ));
    md.push_str(&format!(
        "| Win Rate | {} | {} | {} |\n",
        pct(ma.win_rate),
        pct(mb.win_rate),
        delta_pct(ma.win_rate, mb.win_rate)
    ));
    md.push_str(&format!(
        "| Profit Factor | {} | {} | {} |\n",
        f2(ma.profit_factor),
        f2(mb.profit_factor),
        delta_f3(ma.profit_factor, mb.profit_factor)
    ));
    md.push_str(&format!(
        "| Trades | {} | {} | {} |\n",
        ma.trade_count,
        mb.trade_count,
        (mb.trade_count as i64 - ma.trade_count as i64)
    ));
    md.push_str(&format!(
        "| Turnover | {:.1}x | {:.1}x | {:.1}x |\n",
        ma.turnover,
        mb.turnover,
        mb.turnover - ma.turnover
    ));
    md.push('\n');

    md
}

// ─── Helpers ────────────────────────────────────────────────────────

fn format_component(
    label: &str,
    config: &trendlab_core::fingerprint::ComponentConfig,
) -> String {
    let mut s = format!("- **{label}**: `{}`", config.component_type);
    if !config.params.is_empty() {
        let params: Vec<String> = config.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
        s.push_str(&format!(" ({})", params.join(", ")));
    }
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::collections::HashMap;
    use trendlab_core::domain::position::PositionSide;
    use trendlab_core::engine::stickiness::StickinessMetrics;
    use trendlab_core::fingerprint::{ComponentConfig, StrategyConfig};

    use crate::metrics::PerformanceMetrics;

    // ─── Test helpers ────────────────────────────────────────────────

    fn sample_config() -> StrategyConfig {
        StrategyConfig {
            signal: ComponentConfig {
                component_type: "donchian_breakout".into(),
                params: [("lookback".into(), 50.0)].into_iter().collect(),
            },
            position_manager: ComponentConfig {
                component_type: "atr_trailing".into(),
                params: [("atr_period".into(), 14.0), ("multiplier".into(), 3.0)]
                    .into_iter()
                    .collect(),
            },
            execution_model: ComponentConfig {
                component_type: "next_bar_open".into(),
                params: [("preset".into(), 1.0)].into_iter().collect(),
            },
            signal_filter: ComponentConfig {
                component_type: "no_filter".into(),
                params: Default::default(),
            },
        }
    }

    fn sample_trade() -> TradeRecord {
        TradeRecord {
            symbol: "SPY".into(),
            side: PositionSide::Long,
            entry_bar: 55,
            entry_date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            entry_price: 450.50,
            exit_bar: 72,
            exit_date: NaiveDate::from_ymd_opt(2024, 4, 10).unwrap(),
            exit_price: 468.25,
            quantity: 222.0,
            gross_pnl: 3939.50,
            commission: 20.0,
            slippage: 10.0,
            net_pnl: 3909.50,
            bars_held: 17,
            mae: -500.0,
            mfe: 4200.0,
            signal_id: None,
            signal_type: Some("donchian_breakout".into()),
            pm_type: Some("atr_trailing".into()),
            execution_model: Some("next_bar_open".into()),
            filter_type: Some("no_filter".into()),
        }
    }

    fn sample_result() -> BacktestResult {
        BacktestResult {
            schema_version: SCHEMA_VERSION,
            metrics: PerformanceMetrics {
                total_return: 0.15,
                cagr: 0.12,
                sharpe: 1.25,
                sortino: 1.80,
                calmar: 2.10,
                max_drawdown: -0.08,
                win_rate: 0.45,
                profit_factor: 2.3,
                trade_count: 25,
                turnover: 3.5,
                max_consecutive_wins: 5,
                max_consecutive_losses: 3,
                avg_losing_streak: 1.8,
            },
            trades: vec![sample_trade()],
            equity_curve: vec![100_000.0, 100_500.0, 101_200.0, 103_000.0, 115_000.0],
            config: sample_config(),
            symbol: "SPY".into(),
            start_date: "2024-01-02".into(),
            end_date: "2024-12-31".into(),
            initial_capital: 100_000.0,
            dataset_hash: "abc123".into(),
            has_synthetic: false,
            signal_count: 30,
            bar_count: 252,
            warmup_bars: 50,
            void_bar_rates: HashMap::new(),
            data_quality_warnings: vec![],
            stickiness: None,
        }
    }

    fn sample_result_b() -> BacktestResult {
        let mut r = sample_result();
        r.symbol = "QQQ".into();
        r.config.signal.component_type = "bollinger_breakout".into();
        r.metrics.sharpe = 0.85;
        r.metrics.total_return = 0.10;
        r.metrics.cagr = 0.08;
        r.metrics.max_drawdown = -0.12;
        r.metrics.trade_count = 40;
        r
    }

    // ─── JSON round-trip ─────────────────────────────────────────────

    #[test]
    fn json_roundtrip() {
        let original = sample_result();
        let json = export_json(&original).unwrap();
        let restored = import_json(&json).unwrap();

        assert_eq!(restored.schema_version, SCHEMA_VERSION);
        assert_eq!(restored.symbol, original.symbol);
        assert_eq!(restored.metrics.trade_count, original.metrics.trade_count);
        assert!((restored.metrics.sharpe - original.metrics.sharpe).abs() < 1e-10);
        assert_eq!(restored.trades.len(), original.trades.len());
        assert_eq!(restored.equity_curve.len(), original.equity_curve.len());
        assert_eq!(restored.config, original.config);
        assert_eq!(restored.dataset_hash, original.dataset_hash);
    }

    #[test]
    fn json_rejects_unknown_version() {
        let mut result = sample_result();
        result.schema_version = 99;
        let json = export_json(&result).unwrap();
        let err = import_json(&json);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("unsupported schema version 99"));
    }

    #[test]
    fn json_accepts_current_version() {
        let result = sample_result();
        let json = export_json(&result).unwrap();
        assert!(import_json(&json).is_ok());
    }

    // ─── CSV trades ─────────────────────────────────────────────────

    #[test]
    fn csv_trades_all_columns() {
        let trades = vec![sample_trade()];
        let csv = export_trades_csv(&trades).unwrap();
        let header = csv.lines().next().unwrap();
        let cols: Vec<&str> = header.split(',').collect();

        assert_eq!(cols.len(), 20);
        assert!(cols.contains(&"symbol"));
        assert!(cols.contains(&"side"));
        assert!(cols.contains(&"entry_bar"));
        assert!(cols.contains(&"entry_date"));
        assert!(cols.contains(&"entry_price"));
        assert!(cols.contains(&"exit_bar"));
        assert!(cols.contains(&"exit_date"));
        assert!(cols.contains(&"exit_price"));
        assert!(cols.contains(&"quantity"));
        assert!(cols.contains(&"gross_pnl"));
        assert!(cols.contains(&"commission"));
        assert!(cols.contains(&"slippage"));
        assert!(cols.contains(&"net_pnl"));
        assert!(cols.contains(&"bars_held"));
        assert!(cols.contains(&"mae"));
        assert!(cols.contains(&"mfe"));
        assert!(cols.contains(&"signal_type"));
        assert!(cols.contains(&"pm_type"));
        assert!(cols.contains(&"execution_model"));
        assert!(cols.contains(&"filter_type"));
    }

    #[test]
    fn csv_trades_content() {
        let trade = sample_trade();
        let csv = export_trades_csv(&[trade]).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines.len(), 2); // header + 1 data row
        let row = lines[1];
        assert!(row.contains("SPY"));
        assert!(row.contains("donchian_breakout"));
        assert!(row.contains("atr_trailing"));
        assert!(row.contains("next_bar_open"));
        assert!(row.contains("no_filter"));
        assert!(row.contains("3909.50"));
    }

    #[test]
    fn csv_empty_trades() {
        let csv = export_trades_csv(&[]).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1); // header only
    }

    // ─── CSV equity ─────────────────────────────────────────────────

    #[test]
    fn csv_equity_basic() {
        let eq = vec![100_000.0, 101_000.0, 99_500.0];
        let csv = export_equity_csv(&eq).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines.len(), 4); // header + 3 rows
        assert_eq!(lines[0], "bar_index,equity");
        assert!(lines[1].starts_with("0,100000.00"));
        assert!(lines[2].starts_with("1,101000.00"));
        assert!(lines[3].starts_with("2,99500.00"));
    }

    // ─── Markdown report ────────────────────────────────────────────

    #[test]
    fn markdown_report_has_sections() {
        let result = sample_result();
        let md = generate_report(&result);

        assert!(md.contains("# Backtest Report"));
        assert!(md.contains("## Metadata"));
        assert!(md.contains("## Strategy Composition"));
        assert!(md.contains("## Performance Summary"));
        assert!(md.contains("| Sharpe | 1.250 |"));
        assert!(md.contains("donchian_breakout"));
        assert!(md.contains("atr_trailing"));
    }

    #[test]
    fn markdown_report_with_stickiness() {
        let mut result = sample_result();
        result.stickiness = Some(StickinessMetrics {
            median_holding_bars: 25.0,
            p95_holding_bars: 80.0,
            pct_over_60_bars: 0.12,
            pct_over_120_bars: 0.03,
            exit_trigger_rate: 0.45,
            reference_chase_ratio: 2.2,
        });
        let md = generate_report(&result);

        assert!(md.contains("## Stickiness Diagnostics"));
        assert!(md.contains("Median Holding"));
        assert!(md.contains("Exit Trigger Rate"));
    }

    #[test]
    fn markdown_report_without_stickiness() {
        let result = sample_result();
        let md = generate_report(&result);
        assert!(!md.contains("Stickiness Diagnostics"));
    }

    // ─── Markdown comparison ────────────────────────────────────────

    #[test]
    fn comparison_report_has_delta() {
        let a = sample_result();
        let b = sample_result_b();
        let md = generate_comparison(&a, &b);

        assert!(md.contains("# Strategy Comparison"));
        assert!(md.contains("## Composition"));
        assert!(md.contains("## Performance Comparison"));
        assert!(md.contains("| Delta |"));
        assert!(md.contains("Strategy A"));
        assert!(md.contains("Strategy B"));
        assert!(md.contains("donchian_breakout"));
        assert!(md.contains("bollinger_breakout"));
    }

    #[test]
    fn comparison_report_different_symbols() {
        let a = sample_result();
        let b = sample_result_b();
        let md = generate_comparison(&a, &b);

        assert!(md.contains("| SPY |"));
        assert!(md.contains("| QQQ |"));
    }

    // ─── Save/load artifacts ────────────────────────────────────────

    #[test]
    fn save_load_artifacts_roundtrip() {
        let result = sample_result();
        let dir = tempfile::tempdir().unwrap();
        let run_dir = save_artifacts(&result, dir.path()).unwrap();

        // Verify files exist
        assert!(run_dir.join("manifest.json").exists());
        assert!(run_dir.join("trades.csv").exists());
        assert!(run_dir.join("equity.csv").exists());

        // Round-trip manifest
        let loaded = load_artifacts(&run_dir).unwrap();
        assert_eq!(loaded.symbol, result.symbol);
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert!((loaded.metrics.sharpe - result.metrics.sharpe).abs() < 1e-10);
    }

    // ─── Export coverage ────────────────────────────────────────────

    #[test]
    fn all_export_formats_succeed() {
        let result = sample_result();

        // JSON
        let json = export_json(&result);
        assert!(json.is_ok());

        // Trades CSV
        let csv = export_trades_csv(&result.trades);
        assert!(csv.is_ok());

        // Equity CSV
        let eq = export_equity_csv(&result.equity_curve);
        assert!(eq.is_ok());

        // Markdown report
        let md = generate_report(&result);
        assert!(!md.is_empty());

        // Markdown comparison
        let cmp = generate_comparison(&result, &sample_result_b());
        assert!(!cmp.is_empty());
    }
}
