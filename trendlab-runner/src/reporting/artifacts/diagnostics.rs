//! Diagnostics export (JSON).

use anyhow::{Context, Result};
use std::path::Path;
use crate::result::BacktestResult;

pub fn write_diagnostics_json(path: &Path, result: &BacktestResult) -> Result<()> {
    let mut diagnostics = serde_json::Map::new();

    // Include explicit diagnostics if present
    if let Some(value) = result.metadata.custom.get("diagnostics") {
        diagnostics.insert("raw_diagnostics".to_string(), value.clone());
    }

    // Include rejected intents summary
    if let Some(rejected) = result.metadata.custom.get("rejected_intents") {
        if let Some(arr) = rejected.as_array() {
            diagnostics.insert("rejected_intents".to_string(), rejected.clone());
            diagnostics.insert(
                "rejected_intents_count".to_string(),
                serde_json::json!(arr.len()),
            );

            // Count by reason
            let mut by_reason = serde_json::Map::new();
            for entry in arr {
                if let Some(reason) = entry.get("reason").and_then(|v| v.as_str()) {
                    let count = by_reason
                        .entry(reason.to_string())
                        .or_insert_with(|| serde_json::json!(0));
                    if let Some(n) = count.as_u64() {
                        *count = serde_json::json!(n + 1);
                    }
                }
            }
            diagnostics.insert(
                "rejections_by_type".to_string(),
                serde_json::Value::Object(by_reason),
            );
        }
    }

    // Include ghost curve drag if ideal equity is available
    if let Some(ideal) = result.metadata.custom.get("ideal_equity_curve") {
        if let Some(ideal_arr) = ideal.as_array() {
            if let Some(ideal_last) = ideal_arr.last() {
                let ideal_final = ideal_last
                    .get("equity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let real_final = result.stats.final_equity;
                if ideal_final > 0.0 {
                    let drag_pct = (ideal_final - real_final) / ideal_final * 100.0;
                    diagnostics.insert(
                        "execution_drag_pct".to_string(),
                        serde_json::json!(format!("{:.2}", drag_pct)),
                    );
                    diagnostics.insert(
                        "execution_drag_dollars".to_string(),
                        serde_json::json!(format!("{:.2}", ideal_final - real_final)),
                    );
                }
            }
        }
    }

    // Include performance stats
    diagnostics.insert(
        "performance".to_string(),
        serde_json::json!({
            "sharpe": result.stats.sharpe,
            "sortino": result.stats.sortino,
            "total_return": result.stats.total_return,
            "max_drawdown": result.stats.max_drawdown,
            "win_rate": result.stats.win_rate,
            "profit_factor": result.stats.profit_factor,
            "num_trades": result.stats.num_trades,
        }),
    );

    let output = if diagnostics.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string_pretty(&serde_json::Value::Object(diagnostics))
            .context("Failed to serialize diagnostics")?
    };

    std::fs::write(path, output)
        .with_context(|| format!("Failed to write diagnostics {}", path.display()))?;
    Ok(())
}
