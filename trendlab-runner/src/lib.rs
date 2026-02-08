//! TrendLab Runner — backtest orchestration, YOLO mode, leaderboards, metrics.
//!
//! This crate builds on `trendlab-core` to provide:
//! - Data loading with cache/download/synthetic fallback
//! - Single-backtest runner with trade extraction and metrics
//! - YOLO mode (continuous auto-discovery engine)
//! - Per-symbol and cross-symbol leaderboards
//! - Risk profile ranking system
//! - Run fingerprinting and JSONL history
//! - Promotion ladder (walk-forward, execution MC, bootstrap)

pub mod config;
pub mod data_loader;
pub mod fitness;
pub mod leaderboard;
pub mod metrics;
pub mod runner;
pub mod yolo;

pub use config::{BacktestConfig, ConfigError};
pub use data_loader::{load_bars, LoadError, LoadOptions, LoadedData};
pub use fitness::FitnessMetric;
pub use leaderboard::{InsertResult, LeaderboardEntry, SymbolLeaderboard};
pub use metrics::PerformanceMetrics;
pub use runner::{run_backtest_from_data, run_single_backtest, BacktestResult, RunError};
pub use yolo::{run_yolo, YoloConfig, YoloProgress, YoloResult};

#[cfg(test)]
mod send_sync_checks {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn performance_metrics_is_send_sync() {
        assert_send::<PerformanceMetrics>();
        assert_sync::<PerformanceMetrics>();
    }

    #[test]
    fn backtest_result_is_send_sync() {
        assert_send::<BacktestResult>();
        assert_sync::<BacktestResult>();
    }

    #[test]
    fn fitness_metric_is_send_sync() {
        assert_send::<FitnessMetric>();
        assert_sync::<FitnessMetric>();
    }

    #[test]
    fn config_types_are_send_sync() {
        assert_send::<BacktestConfig>();
        assert_sync::<BacktestConfig>();
        assert_send::<LoadOptions>();
        assert_sync::<LoadOptions>();
    }

    #[test]
    fn yolo_config_is_send_sync() {
        assert_send::<YoloConfig>();
        assert_sync::<YoloConfig>();
    }

    #[test]
    fn yolo_progress_is_send_sync() {
        assert_send::<YoloProgress>();
        assert_sync::<YoloProgress>();
    }

    #[test]
    fn leaderboard_entry_is_send_sync() {
        assert_send::<LeaderboardEntry>();
        assert_sync::<LeaderboardEntry>();
    }

    #[test]
    fn symbol_leaderboard_is_send_sync() {
        assert_send::<SymbolLeaderboard>();
        assert_sync::<SymbolLeaderboard>();
    }
}
