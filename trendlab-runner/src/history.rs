//! YOLO history — JSONL append-only persistence with write filtering.
//!
//! Persists run fingerprints and metrics as one JSON object per line.
//! A write filter prevents junk configurations from bloating the history file
//! during long YOLO sessions (50,000+ iterations).
//!
//! The history enables meta-analysis: "which signal type contributes most to
//! performance across all tested configurations?"

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::metrics::PerformanceMetrics;
use trendlab_core::fingerprint::RunFingerprint;

/// A single history entry: fingerprint + metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub fingerprint: RunFingerprint,
    pub metrics: PerformanceMetrics,
    pub trade_count: usize,
    pub fitness_score: f64,
}

/// Criteria for whether a run should be persisted to the history file.
///
/// Default: at least 5 trades AND (positive CAGR OR Sharpe > -1.0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFilter {
    pub min_trades: usize,
    pub min_cagr: Option<f64>,
    pub min_sharpe: Option<f64>,
}

impl Default for WriteFilter {
    fn default() -> Self {
        Self {
            min_trades: 5,
            min_cagr: Some(0.0),
            min_sharpe: Some(-1.0),
        }
    }
}

impl WriteFilter {
    /// Check whether a run meets the write criteria.
    ///
    /// Logic: `trade_count >= min_trades AND (cagr >= min_cagr OR sharpe >= min_sharpe)`.
    pub fn should_write(&self, metrics: &PerformanceMetrics, trade_count: usize) -> bool {
        if trade_count < self.min_trades {
            return false;
        }
        let cagr_ok = self.min_cagr.map_or(true, |min| metrics.cagr >= min);
        let sharpe_ok = self.min_sharpe.map_or(true, |min| metrics.sharpe >= min);
        cagr_ok || sharpe_ok
    }
}

/// JSONL history file manager.
///
/// Appends filtered entries to a JSONL file. Each line is an independent JSON
/// object, making the format resilient to partial writes and easy to stream.
pub struct YoloHistory {
    path: PathBuf,
    filter: WriteFilter,
}

impl YoloHistory {
    pub fn new(path: PathBuf, filter: WriteFilter) -> Self {
        Self { path, filter }
    }

    /// Append an entry to the history file if it passes the write filter.
    ///
    /// Returns `Ok(true)` if the entry was written, `Ok(false)` if filtered out.
    pub fn append(&self, entry: &HistoryEntry) -> io::Result<bool> {
        if !self.filter.should_write(&entry.metrics, entry.trade_count) {
            return Ok(false);
        }

        let json = serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{json}")?;
        file.flush()?;

        Ok(true)
    }

    /// Get the current file size in bytes.
    pub fn file_size_bytes(&self) -> io::Result<u64> {
        match fs::metadata(&self.path) {
            Ok(meta) => Ok(meta.len()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// Read all entries from the history file.
    ///
    /// Skips malformed lines (logged but not fatal).
    pub fn read_all(&self) -> io::Result<Vec<HistoryEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.path)?;
        let reader = io::BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<HistoryEntry>(&line) {
                Ok(entry) => entries.push(entry),
                Err(_) => continue, // skip malformed lines
            }
        }

        Ok(entries)
    }

    /// Path to the history file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Statistical summary for a component type (signal, PM, execution, filter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSummary {
    pub count: usize,
    pub mean_sharpe: f64,
    pub median_sharpe: f64,
    pub p25_sharpe: f64,
    pub p75_sharpe: f64,
    pub mean_win_rate: f64,
}

/// Compute per-component-type summaries from history entries.
///
/// Groups entries by the specified component field and computes summary statistics.
pub fn summary_by_signal_type(entries: &[HistoryEntry]) -> HashMap<String, ComponentSummary> {
    group_and_summarize(entries, |e| {
        e.fingerprint.strategy_config.signal.component_type.clone()
    })
}

/// Compute per-PM-type summaries from history entries.
pub fn summary_by_pm_type(entries: &[HistoryEntry]) -> HashMap<String, ComponentSummary> {
    group_and_summarize(entries, |e| {
        e.fingerprint
            .strategy_config
            .position_manager
            .component_type
            .clone()
    })
}

/// Compute per-execution-model summaries from history entries.
pub fn summary_by_execution_type(entries: &[HistoryEntry]) -> HashMap<String, ComponentSummary> {
    group_and_summarize(entries, |e| {
        e.fingerprint
            .strategy_config
            .execution_model
            .component_type
            .clone()
    })
}

/// Compute per-filter-type summaries from history entries.
pub fn summary_by_filter_type(entries: &[HistoryEntry]) -> HashMap<String, ComponentSummary> {
    group_and_summarize(entries, |e| {
        e.fingerprint
            .strategy_config
            .signal_filter
            .component_type
            .clone()
    })
}

fn group_and_summarize<F>(entries: &[HistoryEntry], key_fn: F) -> HashMap<String, ComponentSummary>
where
    F: Fn(&HistoryEntry) -> String,
{
    let mut groups: HashMap<String, Vec<&HistoryEntry>> = HashMap::new();

    for entry in entries {
        let key = key_fn(entry);
        groups.entry(key).or_default().push(entry);
    }

    groups
        .into_iter()
        .map(|(key, group)| {
            let mut sharpes: Vec<f64> = group.iter().map(|e| e.metrics.sharpe).collect();
            sharpes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let n = sharpes.len();
            let mean_sharpe = sharpes.iter().sum::<f64>() / n as f64;
            let median_sharpe = if n % 2 == 0 {
                (sharpes[n / 2 - 1] + sharpes[n / 2]) / 2.0
            } else {
                sharpes[n / 2]
            };
            let p25_sharpe = sharpes[n / 4];
            let p75_sharpe = sharpes[3 * n / 4];
            let mean_win_rate = group.iter().map(|e| e.metrics.win_rate).sum::<f64>() / n as f64;

            (
                key,
                ComponentSummary {
                    count: n,
                    mean_sharpe,
                    median_sharpe,
                    p25_sharpe,
                    p75_sharpe,
                    mean_win_rate,
                },
            )
        })
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::collections::BTreeMap;
    use tempfile::TempDir;
    use trendlab_core::domain::{DatasetHash, RunId};
    use trendlab_core::fingerprint::{ComponentConfig, StrategyConfig, TradingMode};

    fn make_fingerprint(signal_type: &str, sharpe: f64) -> (RunFingerprint, PerformanceMetrics) {
        let config = StrategyConfig {
            signal: ComponentConfig {
                component_type: signal_type.into(),
                params: BTreeMap::new(),
            },
            position_manager: ComponentConfig {
                component_type: "atr_trailing".into(),
                params: BTreeMap::new(),
            },
            execution_model: ComponentConfig {
                component_type: "next_bar_open".into(),
                params: BTreeMap::new(),
            },
            signal_filter: ComponentConfig {
                component_type: "no_filter".into(),
                params: BTreeMap::new(),
            },
        };

        let fp = RunFingerprint {
            run_id: RunId::from_bytes(b"test"),
            timestamp: chrono::Utc::now().naive_utc(),
            seed: 42,
            symbol: "SPY".into(),
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            trading_mode: TradingMode::LongOnly,
            initial_capital: 100_000.0,
            strategy_config: config.clone(),
            config_hash: config.config_hash(),
            full_hash: config.full_hash(),
            dataset_hash: DatasetHash::from_bytes(b"test"),
        };

        let metrics = PerformanceMetrics {
            total_return: 0.10,
            cagr: 0.08,
            sharpe,
            sortino: 1.0,
            calmar: 0.5,
            max_drawdown: -0.10,
            win_rate: 0.55,
            profit_factor: 1.5,
            trade_count: 20,
            turnover: 2.0,
            max_consecutive_wins: 4,
            max_consecutive_losses: 3,
            avg_losing_streak: 1.5,
        };

        (fp, metrics)
    }

    #[test]
    fn write_filter_rejects_too_few_trades() {
        let filter = WriteFilter::default(); // min_trades = 5
        let (_, metrics) = make_fingerprint("donchian", 1.5);
        assert!(!filter.should_write(&metrics, 3));
    }

    #[test]
    fn write_filter_accepts_enough_trades_positive_cagr() {
        let filter = WriteFilter::default();
        let (_, metrics) = make_fingerprint("donchian", 1.5);
        assert!(filter.should_write(&metrics, 10));
    }

    #[test]
    fn write_filter_accepts_negative_cagr_positive_sharpe() {
        let filter = WriteFilter::default();
        let (_, mut metrics) = make_fingerprint("donchian", 0.5);
        metrics.cagr = -0.05; // negative CAGR
                              // Sharpe = 0.5 > -1.0, so should_write = true (OR logic)
        assert!(filter.should_write(&metrics, 10));
    }

    #[test]
    fn write_filter_rejects_bad_cagr_and_bad_sharpe() {
        let filter = WriteFilter::default();
        let (_, mut metrics) = make_fingerprint("donchian", -1.5);
        metrics.cagr = -0.10;
        // Both below thresholds
        assert!(!filter.should_write(&metrics, 10));
    }

    #[test]
    fn append_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.jsonl");
        let history = YoloHistory::new(path, WriteFilter::default());

        let (fp, metrics) = make_fingerprint("donchian", 1.5);
        let entry = HistoryEntry {
            fingerprint: fp,
            metrics,
            trade_count: 20,
            fitness_score: 1.5,
        };

        let written = history.append(&entry).unwrap();
        assert!(written);

        let entries = history.read_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert!((entries[0].fitness_score - 1.5).abs() < 1e-10);
    }

    #[test]
    fn append_filtered_entry_not_written() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.jsonl");
        let history = YoloHistory::new(path, WriteFilter::default());

        let (fp, mut metrics) = make_fingerprint("donchian", -2.0);
        metrics.cagr = -0.50;
        let entry = HistoryEntry {
            fingerprint: fp,
            metrics,
            trade_count: 20,
            fitness_score: -2.0,
        };

        let written = history.append(&entry).unwrap();
        assert!(!written);

        let entries = history.read_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn file_size_tracking() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.jsonl");
        let history = YoloHistory::new(path, WriteFilter::default());

        assert_eq!(history.file_size_bytes().unwrap(), 0);

        let (fp, metrics) = make_fingerprint("donchian", 1.5);
        let entry = HistoryEntry {
            fingerprint: fp,
            metrics,
            trade_count: 20,
            fitness_score: 1.5,
        };
        history.append(&entry).unwrap();

        let size = history.file_size_bytes().unwrap();
        assert!(size > 0);
    }

    #[test]
    fn multiple_appends() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.jsonl");
        let history = YoloHistory::new(path, WriteFilter::default());

        for i in 0..5 {
            let (fp, metrics) = make_fingerprint("donchian", 1.0 + i as f64 * 0.5);
            let entry = HistoryEntry {
                fingerprint: fp,
                metrics,
                trade_count: 20,
                fitness_score: 1.0 + i as f64 * 0.5,
            };
            history.append(&entry).unwrap();
        }

        let entries = history.read_all().unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn read_nonexistent_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("does_not_exist.jsonl");
        let history = YoloHistory::new(path, WriteFilter::default());

        let entries = history.read_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn summary_by_signal_type_groups_correctly() {
        let (fp1, m1) = make_fingerprint("donchian", 1.5);
        let (fp2, m2) = make_fingerprint("donchian", 2.0);
        let (fp3, m3) = make_fingerprint("bollinger", 1.0);

        let entries = vec![
            HistoryEntry {
                fingerprint: fp1,
                metrics: m1,
                trade_count: 20,
                fitness_score: 1.5,
            },
            HistoryEntry {
                fingerprint: fp2,
                metrics: m2,
                trade_count: 20,
                fitness_score: 2.0,
            },
            HistoryEntry {
                fingerprint: fp3,
                metrics: m3,
                trade_count: 20,
                fitness_score: 1.0,
            },
        ];

        let summary = summary_by_signal_type(&entries);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary["donchian"].count, 2);
        assert!((summary["donchian"].mean_sharpe - 1.75).abs() < 1e-10);
        assert_eq!(summary["bollinger"].count, 1);
        assert!((summary["bollinger"].mean_sharpe - 1.0).abs() < 1e-10);
    }

    #[test]
    fn write_filter_serialization() {
        let filter = WriteFilter::default();
        let json = serde_json::to_string(&filter).unwrap();
        let deser: WriteFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.min_trades, 5);
    }
}
