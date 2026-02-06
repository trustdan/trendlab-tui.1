//! Artifact manager for persisting run outputs.

mod manifest;
mod equity;
mod trades;
mod diagnostics;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use crate::result::BacktestResult;

pub use manifest::RunManifest;

/// Artifact paths returned after export.
#[derive(Debug, Clone)]
pub struct ArtifactPaths {
    pub manifest: PathBuf,
    pub equity_csv: PathBuf,
    pub equity_parquet: PathBuf,
    pub trades_csv: PathBuf,
    pub trades_json: PathBuf,
    pub diagnostics_json: PathBuf,
    pub report_markdown: Option<PathBuf>,
}

/// Manages writing all artifacts for a run.
#[derive(Debug, Clone)]
pub struct ArtifactManager {
    output_dir: PathBuf,
}

impl ArtifactManager {
    pub fn new(output_dir: impl AsRef<Path>) -> Result<Self> {
        let output_dir = output_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&output_dir)
            .context("Failed to create artifact output directory")?;
        Ok(Self { output_dir })
    }

    /// Save complete run artifacts.
    pub fn save_run(&self, result: &BacktestResult) -> Result<ArtifactPaths> {
        let run_dir = self.output_dir.join(&result.run_id);
        std::fs::create_dir_all(&run_dir)
            .context("Failed to create run artifact directory")?;

        let manifest_path = run_dir.join("manifest.json");
        manifest::write_manifest(&manifest_path, result)?;

        let equity_csv = run_dir.join("equity.csv");
        let equity_parquet = run_dir.join("equity.parquet");
        equity::write_equity_csv(&equity_csv, &result.equity_curve)?;
        equity::write_equity_parquet(&equity_parquet, &result.equity_curve)?;

        let trades_csv = run_dir.join("trades.csv");
        let trades_json = run_dir.join("trades.json");
        trades::write_trades_csv(&trades_csv, &result.trades)?;
        trades::write_trades_json(&trades_json, &result.trades)?;

        let diagnostics_json = run_dir.join("diagnostics.json");
        diagnostics::write_diagnostics_json(&diagnostics_json, result)?;

        Ok(ArtifactPaths {
            manifest: manifest_path,
            equity_csv,
            equity_parquet,
            trades_csv,
            trades_json,
            diagnostics_json,
            report_markdown: None,
        })
    }
}
