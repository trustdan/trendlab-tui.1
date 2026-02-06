//! Trade tape export (CSV/JSON).

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::result::{TradeRecord, TradeDirection};

pub fn write_trades_csv(path: &Path, trades: &[TradeRecord]) -> Result<()> {
    let mut file = File::create(path)
        .with_context(|| format!("Failed to create trades CSV {}", path.display()))?;

    writeln!(
        file,
        "symbol,entry_date,exit_date,direction,entry_price,exit_price,quantity,pnl,return_pct"
    )?;

    for trade in trades {
        let direction = match trade.direction {
            TradeDirection::Long => "Long",
            TradeDirection::Short => "Short",
        };
        writeln!(
            file,
            "{},{},{},{},{:.4},{:.4},{},{:.4},{:.4}",
            trade.symbol,
            trade.entry_date,
            trade.exit_date,
            direction,
            trade.entry_price,
            trade.exit_price,
            trade.quantity,
            trade.pnl,
            trade.return_pct
        )?;
    }

    Ok(())
}

pub fn write_trades_json(path: &Path, trades: &[TradeRecord]) -> Result<()> {
    let json = serde_json::to_string_pretty(trades)
        .context("Failed to serialize trades")?;
    std::fs::write(path, json)
        .with_context(|| format!("Failed to write trades JSON {}", path.display()))?;
    Ok(())
}
