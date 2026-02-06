//! Markdown report generator.

use crate::result::BacktestResult;
use super::SummaryStats;

pub struct MarkdownReportGenerator;

impl MarkdownReportGenerator {
    pub fn generate(&self, result: &BacktestResult) -> String {
        let summary = SummaryStats::from_result(result);
        let mut report = format!(
            "# TrendLab Run Report\n\n\
Run ID: `{}`\n\n\
## Summary\n\
- Sharpe: {:.2}\n\
- Total Return: {:+.2}%\n\
- Max Drawdown: {:+.2}%\n\
- Win Rate: {:.1}%\n\
- Trades: {}\n",
            result.run_id,
            summary.sharpe,
            summary.total_return * 100.0,
            summary.max_drawdown * 100.0,
            summary.win_rate * 100.0,
            summary.num_trades
        );

        // Trade tape section (top 5 winners and losers)
        if !result.trades.is_empty() {
            report.push_str("\n## Trade Tape\n\n");

            let mut sorted_trades: Vec<_> = result.trades.iter().collect();
            sorted_trades.sort_by(|a, b| {
                b.pnl
                    .partial_cmp(&a.pnl)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Top winners
            report.push_str("### Top Winners\n");
            report.push_str("| Symbol | Direction | Entry | Exit | PnL | Return |\n");
            report.push_str("|--------|-----------|-------|------|-----|--------|\n");
            for trade in sorted_trades.iter().take(5).filter(|t| t.pnl > 0.0) {
                report.push_str(&format!(
                    "| {} | {:?} | {} | {} | ${:+.2} | {:+.2}% |\n",
                    trade.symbol,
                    trade.direction,
                    trade.entry_date,
                    trade.exit_date,
                    trade.pnl,
                    trade.return_pct
                ));
            }

            // Top losers
            report.push_str("\n### Top Losers\n");
            report.push_str("| Symbol | Direction | Entry | Exit | PnL | Return |\n");
            report.push_str("|--------|-----------|-------|------|-----|--------|\n");
            for trade in sorted_trades.iter().rev().take(5).filter(|t| t.pnl <= 0.0) {
                report.push_str(&format!(
                    "| {} | {:?} | {} | {} | ${:+.2} | {:+.2}% |\n",
                    trade.symbol,
                    trade.direction,
                    trade.entry_date,
                    trade.exit_date,
                    trade.pnl,
                    trade.return_pct
                ));
            }
        }

        // Rejection stats section
        if let Some(rejected) = result.metadata.custom.get("rejected_intents") {
            if let Some(arr) = rejected.as_array() {
                if !arr.is_empty() {
                    report.push_str("\n## Rejected Intents\n\n");

                    let total_signals = result
                        .metadata
                        .custom
                        .get("total_signals")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    report.push_str(&format!("- Total signals: {}\n", total_signals));
                    report.push_str(&format!("- Rejected: {}\n", arr.len()));
                    if total_signals > 0 {
                        let reject_rate = arr.len() as f64 / total_signals as f64 * 100.0;
                        report.push_str(&format!("- Rejection rate: {:.1}%\n", reject_rate));
                    }

                    // Count by type
                    let mut by_type = std::collections::HashMap::new();
                    for entry in arr {
                        if let Some(reason) = entry.get("reason").and_then(|v| v.as_str()) {
                            *by_type.entry(reason.to_string()).or_insert(0usize) += 1;
                        }
                    }
                    report.push_str("\n| Guard | Count |\n");
                    report.push_str("|-------|-------|\n");
                    for (reason, count) in &by_type {
                        report.push_str(&format!("| {} | {} |\n", reason, count));
                    }
                }
            }
        }

        // Execution drag section
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
                        let drag_dollars = ideal_final - real_final;
                        report.push_str("\n## Execution Drag\n\n");
                        report
                            .push_str(&format!("- Ideal final equity: ${:.2}\n", ideal_final));
                        report
                            .push_str(&format!("- Real final equity: ${:.2}\n", real_final));
                        report.push_str(&format!(
                            "- Drag: {:.2}% (${:.2})\n",
                            drag_pct, drag_dollars
                        ));
                        if drag_pct > 15.0 {
                            report.push_str(
                                "- **DEATH CROSSING**: Drag exceeds 15% threshold\n",
                            );
                        }
                    }
                }
            }
        }

        report.push_str(
            "\n## Notes\n\
- Equity curve and trades are exported alongside this report.\n",
        );

        report
    }
}
