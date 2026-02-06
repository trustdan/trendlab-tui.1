//! Run manifest export (JSON).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::result::{BacktestResult, PerformanceStats};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration_secs: f64,
    pub stats: PerformanceStats,
}

pub fn write_manifest(path: &Path, result: &BacktestResult) -> Result<()> {
    let manifest = RunManifest {
        run_id: result.run_id.clone(),
        timestamp: result.metadata.timestamp,
        duration_secs: result.metadata.duration_secs,
        stats: result.stats.clone(),
    };

    let json = serde_json::to_string_pretty(&manifest)
        .context("Failed to serialize run manifest")?;
    std::fs::write(path, json)
        .with_context(|| format!("Failed to write manifest to {}", path.display()))?;
    Ok(())
}
