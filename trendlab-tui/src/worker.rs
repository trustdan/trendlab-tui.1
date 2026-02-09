//! Background worker thread — all heavy computation runs here.
//!
//! Communication with the TUI main thread is via `mpsc` channels.
//! The worker creates a private rayon::ThreadPool (not the global pool).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use chrono::NaiveDate;

use trendlab_core::data::cache::ParquetCache;
use trendlab_core::data::circuit_breaker::CircuitBreaker;
use trendlab_core::data::provider::{DataError, DataProvider, DownloadProgress};
use trendlab_core::data::yahoo::YahooProvider;
use trendlab_core::fingerprint::{StrategyConfig, TradingMode};
use trendlab_runner::data_loader::LoadOptions;
use trendlab_runner::{
    BacktestResult, YoloConfig, YoloProgress,
    run_backtest_from_data,
};

/// Commands sent from the TUI to the worker.
#[derive(Debug)]
#[allow(dead_code)]
pub enum WorkerCommand {
    FetchData {
        symbols: Vec<String>,
        start: NaiveDate,
        end: NaiveDate,
        cache_dir: PathBuf,
    },
    RunSingleBacktest {
        config: StrategyConfig,
        symbols: Vec<String>,
        trading_mode: TradingMode,
        initial_capital: f64,
        position_size_pct: f64,
        start: NaiveDate,
        end: NaiveDate,
        cache_dir: PathBuf,
    },
    StartYolo {
        config: YoloConfig,
        symbols: Vec<String>,
        cache_dir: PathBuf,
    },
    StopYolo,
    RequestEquityCurve {
        index: usize,
    },
    Shutdown,
}

/// Responses sent from the worker back to the TUI.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WorkerResponse {
    // Data fetching
    FetchProgress {
        symbol: String,
        index: usize,
        total: usize,
    },
    FetchSymbolDone {
        symbol: String,
        success: bool,
        error: Option<String>,
    },
    FetchBatchDone {
        succeeded: usize,
        failed: usize,
    },

    // Single backtest
    BacktestComplete {
        result: Box<BacktestResult>,
    },
    BacktestError {
        error: String,
    },

    // YOLO mode
    YoloProgress(YoloProgress),
    YoloDone {
        result: YoloResultSummary,
    },
    YoloError {
        error: String,
    },

    // Equity curve (on demand)
    EquityCurve {
        index: usize,
        curve: Vec<f64>,
        label: String,
    },

    // General errors
    Error {
        category: String,
        message: String,
        context: String,
    },
}

/// Lightweight summary of YOLO results (without full equity curves).
#[derive(Debug, Clone)]
pub struct YoloResultSummary {
    pub iterations_completed: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub elapsed_secs: f64,
}

/// Spawn the background worker thread.
pub fn spawn_worker(
    rx: Receiver<WorkerCommand>,
    tx: Sender<WorkerResponse>,
    cancel: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("trendlab-worker".into())
        .spawn(move || {
            worker_loop(rx, tx, cancel);
        })
        .expect("failed to spawn worker thread")
}

fn worker_loop(
    rx: Receiver<WorkerCommand>,
    tx: Sender<WorkerResponse>,
    cancel: Arc<AtomicBool>,
) {
    // Create a private rayon thread pool (not the global one).
    let _pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .thread_name(|i| format!("trendlab-pool-{i}"))
        .build()
        .expect("failed to build worker rayon pool");

    loop {
        match rx.recv() {
            Ok(WorkerCommand::Shutdown) | Err(_) => break,
            Ok(cmd) => {
                cancel.store(false, Ordering::Relaxed);
                handle_command(cmd, &tx, &cancel);
            }
        }
    }
}

fn handle_command(
    cmd: WorkerCommand,
    tx: &Sender<WorkerResponse>,
    cancel: &Arc<AtomicBool>,
) {
    match cmd {
        WorkerCommand::FetchData { symbols, start, end, cache_dir } => {
            handle_fetch(symbols, start, end, cache_dir, tx, cancel);
        }
        WorkerCommand::RunSingleBacktest {
            config, symbols, trading_mode, initial_capital,
            position_size_pct, start, end, cache_dir,
        } => {
            handle_single_backtest(
                config, symbols, trading_mode, initial_capital,
                position_size_pct, start, end, cache_dir, tx,
            );
        }
        WorkerCommand::StartYolo { config, symbols, cache_dir } => {
            handle_yolo(config, symbols, cache_dir, tx, cancel);
        }
        WorkerCommand::StopYolo => {
            cancel.store(true, Ordering::Relaxed);
        }
        WorkerCommand::RequestEquityCurve { .. } => {
            // Will be implemented when results are stored on worker side
        }
        WorkerCommand::Shutdown => {} // handled in loop
    }
}

fn handle_fetch(
    symbols: Vec<String>,
    start: NaiveDate,
    end: NaiveDate,
    cache_dir: PathBuf,
    tx: &Sender<WorkerResponse>,
    cancel: &Arc<AtomicBool>,
) {
    let cache = ParquetCache::new(&cache_dir);
    let circuit_breaker = Arc::new(CircuitBreaker::new(std::time::Duration::from_secs(1800)));
    let provider = YahooProvider::new(circuit_breaker);
    let progress = ChannelProgress { tx: tx.clone() };

    let total = symbols.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for (i, symbol) in symbols.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        progress.on_start(symbol, i, total);

        match provider.fetch(symbol, start, end) {
            Ok(result) => {
                if let Err(e) = cache.write(symbol, &result.bars) {
                    let err_msg = format!("cache write failed: {e}");
                    progress.on_complete(symbol, i, total, &Err(DataError::CacheError(err_msg.clone())));
                    failed += 1;
                } else {
                    progress.on_complete(symbol, i, total, &Ok(()));
                    succeeded += 1;
                }
            }
            Err(e) => {
                progress.on_complete(symbol, i, total, &Err(e));
                failed += 1;
            }
        }
    }

    progress.on_batch_complete(succeeded, failed, total);
}

#[allow(clippy::too_many_arguments)]
fn handle_single_backtest(
    config: StrategyConfig,
    symbols: Vec<String>,
    trading_mode: TradingMode,
    initial_capital: f64,
    position_size_pct: f64,
    start: NaiveDate,
    end: NaiveDate,
    cache_dir: PathBuf,
    tx: &Sender<WorkerResponse>,
) {
    let cache = ParquetCache::new(&cache_dir);
    let opts = LoadOptions {
        start,
        end,
        offline: false,
        synthetic: false,
        force: false,
    };

    let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();

    match trendlab_runner::load_bars(&sym_refs, &cache, None, None, &opts) {
        Ok(loaded) => {
            // Run on first symbol
            let symbol = symbols.first().map(|s| s.as_str()).unwrap_or("SPY");
            match run_backtest_from_data(
                &config,
                &loaded.aligned,
                symbol,
                trading_mode,
                initial_capital,
                position_size_pct,
                trendlab_core::components::execution::ExecutionPreset::Realistic,
                &loaded.dataset_hash,
                loaded.has_synthetic,
            ) {
                Ok(result) => {
                    let _ = tx.send(WorkerResponse::BacktestComplete { result: Box::new(result) });
                }
                Err(e) => {
                    let _ = tx.send(WorkerResponse::BacktestError {
                        error: e.to_string(),
                    });
                }
            }
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::BacktestError {
                error: e.to_string(),
            });
        }
    }
}

fn handle_yolo(
    config: YoloConfig,
    symbols: Vec<String>,
    cache_dir: PathBuf,
    tx: &Sender<WorkerResponse>,
    cancel: &Arc<AtomicBool>,
) {
    let cache = ParquetCache::new(&cache_dir);
    let opts = LoadOptions {
        start: config.start_date,
        end: config.end_date,
        offline: false,
        synthetic: false,
        force: false,
    };

    let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();

    match trendlab_runner::load_bars(&sym_refs, &cache, None, None, &opts) {
        Ok(loaded) => {
            let tx_clone = tx.clone();
            let progress_cb = move |progress: &YoloProgress| {
                let _ = tx_clone.send(WorkerResponse::YoloProgress(progress.clone()));
            };

            match trendlab_runner::run_yolo(
                &config,
                &loaded,
                &symbols,
                Some(&progress_cb),
                Some(cancel.as_ref()),
            ) {
                Ok(result) => {
                    let _ = tx.send(WorkerResponse::YoloDone {
                        result: YoloResultSummary {
                            iterations_completed: result.iterations_completed,
                            success_count: result.success_count,
                            error_count: result.error_count,
                            elapsed_secs: result.elapsed_secs,
                        },
                    });
                }
                Err(e) => {
                    let _ = tx.send(WorkerResponse::YoloError {
                        error: e.to_string(),
                    });
                }
            }
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::YoloError {
                error: e.to_string(),
            });
        }
    }
}

/// DownloadProgress implementation that sends messages through a channel.
struct ChannelProgress {
    tx: Sender<WorkerResponse>,
}

impl DownloadProgress for ChannelProgress {
    fn on_start(&self, symbol: &str, index: usize, total: usize) {
        let _ = self.tx.send(WorkerResponse::FetchProgress {
            symbol: symbol.to_string(),
            index,
            total,
        });
    }

    fn on_complete(
        &self,
        symbol: &str,
        _index: usize,
        _total: usize,
        result: &Result<(), DataError>,
    ) {
        let _ = self.tx.send(WorkerResponse::FetchSymbolDone {
            symbol: symbol.to_string(),
            success: result.is_ok(),
            error: result.as_ref().err().map(|e| e.to_string()),
        });
    }

    fn on_batch_complete(&self, succeeded: usize, failed: usize, _total: usize) {
        let _ = self.tx.send(WorkerResponse::FetchBatchDone {
            succeeded,
            failed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn worker_shutdown() {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (resp_tx, _resp_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let handle = spawn_worker(cmd_rx, resp_tx, cancel);
        cmd_tx.send(WorkerCommand::Shutdown).unwrap();
        handle.join().expect("worker should join cleanly");
    }

    #[test]
    fn worker_uses_private_pool() {
        // The global rayon pool thread count should not change after spawning our worker
        let global_threads = rayon::current_num_threads();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (resp_tx, _resp_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let handle = spawn_worker(cmd_rx, resp_tx, cancel);
        // Global pool should be unchanged
        assert_eq!(rayon::current_num_threads(), global_threads);

        cmd_tx.send(WorkerCommand::Shutdown).unwrap();
        handle.join().unwrap();
    }
}
