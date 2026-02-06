//! Load backtest results from disk for the TUI.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use trendlab_runner::result::BacktestResult;

/// Configuration for loading results.
#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub results_path: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
}

impl LoadConfig {
    pub fn empty() -> Self {
        Self {
            results_path: None,
            cache_dir: None,
        }
    }
}

/// Load results based on the provided config.
pub fn load_results(config: &LoadConfig) -> Result<Vec<BacktestResult>> {
    if let Some(path) = &config.results_path {
        return load_results_from_path(path);
    }

    if let Some(path) = &config.cache_dir {
        return load_results_from_path(path);
    }

    Ok(Vec::new())
}

fn load_results_from_path(path: &Path) -> Result<Vec<BacktestResult>> {
    if path.is_file() {
        return load_results_from_file(path);
    }

    if path.is_dir() {
        return load_results_from_dir(path);
    }

    Ok(Vec::new())
}

fn load_results_from_file(path: &Path) -> Result<Vec<BacktestResult>> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read results file: {}", path.display()))?;

    if let Ok(results) = serde_json::from_str::<Vec<BacktestResult>>(&data) {
        return Ok(results);
    }

    let result = serde_json::from_str::<BacktestResult>(&data)
        .with_context(|| format!("Failed to parse result JSON: {}", path.display()))?;

    Ok(vec![result])
}

fn load_results_from_dir(path: &Path) -> Result<Vec<BacktestResult>> {
    let mut results = Vec::new();

    for entry in std::fs::read_dir(path)
        .with_context(|| format!("Failed to read results directory: {}", path.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            let data = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;

            let parsed: Value = serde_json::from_str(&data)
                .with_context(|| format!("Failed to parse {}", path.display()))?;

            if parsed.is_array() {
                let mut batch = serde_json::from_value::<Vec<BacktestResult>>(parsed)?;
                results.append(&mut batch);
            } else {
                let result = serde_json::from_value::<BacktestResult>(parsed)?;
                results.push(result);
            }
        }
    }

    Ok(results)
}
