//! Equity curve export (CSV/Parquet).

use anyhow::{Context, Result};
use polars::prelude::{Column, DataFrame, ParquetWriter, Series, NamedFrom};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::result::EquityPoint;

pub fn write_equity_csv(path: &Path, equity: &[EquityPoint]) -> Result<()> {
    let mut file = File::create(path)
        .with_context(|| format!("Failed to create equity CSV {}", path.display()))?;
    writeln!(file, "date,equity")?;
    for point in equity {
        writeln!(file, "{},{:.4}", point.date, point.equity)?;
    }
    Ok(())
}

pub fn write_equity_parquet(path: &Path, equity: &[EquityPoint]) -> Result<()> {
    let dates: Vec<String> = equity.iter().map(|p| p.date.to_string()).collect();
    let values: Vec<f64> = equity.iter().map(|p| p.equity).collect();

    let mut df = DataFrame::new(vec![
        Column::Series(Series::new("date".into(), dates)),
        Column::Series(Series::new("equity".into(), values)),
    ])
    .context("Failed to build equity dataframe")?;

    let mut file = File::create(path)
        .with_context(|| format!("Failed to create equity parquet {}", path.display()))?;
    ParquetWriter::new(&mut file)
        .finish(&mut df)
        .context("Failed to write equity parquet")?;
    Ok(())
}
