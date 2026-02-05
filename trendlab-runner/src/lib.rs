//! # TrendLab Runner
//!
//! Batch execution layer for backtesting strategies.
//!
//! ## Components
//!
//! - `RunConfig`: Serializable configuration for a single backtest
//! - `Runner`: Orchestrates single or batch backtests
//! - `BacktestResult`: Captures equity curve, trades, and statistics
//! - `ParamSweep`: Grid/random search over parameter ranges with parallelization
//! - `Leaderboard`: Ranks strategies by fitness metrics
//! - `Cache`: Parquet-based caching with hash-based deduplication

pub mod cache;
pub mod config;
pub mod leaderboard;
pub mod result;
pub mod runner;
pub mod sweep;

pub use cache::ResultCache;
pub use config::{
    ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, RunConfig, SignalGeneratorConfig,
    StrategyConfig,
};
pub use leaderboard::{FitnessMetric, Leaderboard};
pub use result::BacktestResult;
pub use runner::Runner;
pub use sweep::{ParamGrid, ParamSweep};
