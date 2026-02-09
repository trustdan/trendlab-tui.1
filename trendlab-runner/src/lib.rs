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

pub mod bootstrap;
pub mod config;
pub mod cross_leaderboard;
pub mod data_loader;
pub mod execution_mc;
pub mod fdr;
pub mod fitness;
pub mod history;
pub mod leaderboard;
pub mod metrics;
pub mod promotion;
pub mod risk_profile;
pub mod runner;
pub mod tail_metrics;
pub mod walk_forward;
pub mod yolo;

pub use bootstrap::{
    stationary_block_bootstrap, BootstrapConfig, BootstrapResult, ConfidenceGrade,
    CrossSymbolBootstrapResult, PerSymbolDiagnostic,
};
pub use config::{BacktestConfig, ConfigError};
pub use cross_leaderboard::{AggregatedStickiness, CrossSymbolEntry, CrossSymbolLeaderboard};
pub use data_loader::{load_bars, LoadError, LoadOptions, LoadedData};
pub use execution_mc::{ExecutionMcConfig, ExecutionMcResult, McSample, StabilityScore};
pub use fdr::{benjamini_hochberg, FdrFamily, FdrResult, TTestResult};
pub use fitness::FitnessMetric;
pub use history::{ComponentSummary, HistoryEntry, WriteFilter, YoloHistory};
pub use leaderboard::{InsertResult, LeaderboardEntry, SymbolLeaderboard};
pub use metrics::PerformanceMetrics;
pub use promotion::{PromotionConfig, PromotionLevel, RobustnessResult};
pub use risk_profile::{RankingMetric, RiskProfile};
pub use runner::{run_backtest_from_data, run_single_backtest, BacktestResult, RunError};
pub use tail_metrics::TailMetrics;
pub use walk_forward::{
    DegradationFlag, WalkForwardConfig, WalkForwardResult,
};
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

    #[test]
    fn cross_symbol_entry_is_send_sync() {
        assert_send::<CrossSymbolEntry>();
        assert_sync::<CrossSymbolEntry>();
    }

    #[test]
    fn cross_symbol_leaderboard_is_send_sync() {
        assert_send::<CrossSymbolLeaderboard>();
        assert_sync::<CrossSymbolLeaderboard>();
    }

    #[test]
    fn risk_profile_is_send_sync() {
        assert_send::<RiskProfile>();
        assert_sync::<RiskProfile>();
    }

    #[test]
    fn ranking_metric_is_send_sync() {
        assert_send::<RankingMetric>();
        assert_sync::<RankingMetric>();
    }

    #[test]
    fn tail_metrics_is_send_sync() {
        assert_send::<TailMetrics>();
        assert_sync::<TailMetrics>();
    }

    #[test]
    fn history_entry_is_send_sync() {
        assert_send::<HistoryEntry>();
        assert_sync::<HistoryEntry>();
    }

    #[test]
    fn write_filter_is_send_sync() {
        assert_send::<WriteFilter>();
        assert_sync::<WriteFilter>();
    }

    // ── Phase 11: Robustness types ──

    #[test]
    fn bootstrap_config_is_send_sync() {
        assert_send::<BootstrapConfig>();
        assert_sync::<BootstrapConfig>();
    }

    #[test]
    fn bootstrap_result_is_send_sync() {
        assert_send::<BootstrapResult>();
        assert_sync::<BootstrapResult>();
    }

    #[test]
    fn confidence_grade_is_send_sync() {
        assert_send::<ConfidenceGrade>();
        assert_sync::<ConfidenceGrade>();
    }

    #[test]
    fn execution_mc_config_is_send_sync() {
        assert_send::<ExecutionMcConfig>();
        assert_sync::<ExecutionMcConfig>();
    }

    #[test]
    fn execution_mc_result_is_send_sync() {
        assert_send::<ExecutionMcResult>();
        assert_sync::<ExecutionMcResult>();
    }

    #[test]
    fn stability_score_is_send_sync() {
        assert_send::<StabilityScore>();
        assert_sync::<StabilityScore>();
    }

    #[test]
    fn fdr_family_is_send_sync() {
        assert_send::<FdrFamily>();
        assert_sync::<FdrFamily>();
    }

    #[test]
    fn fdr_result_is_send_sync() {
        assert_send::<FdrResult>();
        assert_sync::<FdrResult>();
    }

    #[test]
    fn t_test_result_is_send_sync() {
        assert_send::<TTestResult>();
        assert_sync::<TTestResult>();
    }

    #[test]
    fn promotion_config_is_send_sync() {
        assert_send::<PromotionConfig>();
        assert_sync::<PromotionConfig>();
    }

    #[test]
    fn robustness_result_is_send_sync() {
        assert_send::<RobustnessResult>();
        assert_sync::<RobustnessResult>();
    }

    #[test]
    fn promotion_level_is_send_sync() {
        assert_send::<PromotionLevel>();
        assert_sync::<PromotionLevel>();
    }

    #[test]
    fn walk_forward_config_is_send_sync() {
        assert_send::<WalkForwardConfig>();
        assert_sync::<WalkForwardConfig>();
    }

    #[test]
    fn walk_forward_result_is_send_sync() {
        assert_send::<WalkForwardResult>();
        assert_sync::<WalkForwardResult>();
    }

    #[test]
    fn aggregated_stickiness_is_send_sync() {
        assert_send::<AggregatedStickiness>();
        assert_sync::<AggregatedStickiness>();
    }
}
