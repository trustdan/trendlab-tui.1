//! TUI panels for different views
//!
//! Core panels:
//! - Leaderboard: Strategy rankings
//! - Chart: Equity curve with ghost overlay
//! - TradeTape: List of trades
//! - RejectedIntents: Blocked signals timeline
//! - ExecutionLab: Execution sensitivity analysis
//! - Sensitivity: Cross-preset comparison table
//! - RunManifest: Full config viewer
//! - CandleChart: OHLC candle rendering
//! - Robustness: Robustness ladder visualization
//! - DistributionChart: Box-and-whisker widget

pub mod leaderboard;
pub mod chart;
pub mod trade_tape;
pub mod rejected_intents;
pub mod execution_lab;
pub mod sensitivity;
pub mod run_manifest;
pub mod candle_chart;
pub mod distribution_chart;
pub mod robustness;

pub use leaderboard::LeaderboardPanel;
pub use chart::ChartPanel;
pub use candle_chart::CandleChartPanel;
pub use trade_tape::TradeTapePanel;
pub use rejected_intents::RejectedIntentsPanel;
pub use execution_lab::ExecutionLabPanel;
pub use sensitivity::SensitivityPanel;
pub use run_manifest::RunManifestPanel;
pub use robustness::RobustnessPanel;
pub use distribution_chart::DistributionChart;
pub use chart::TradeMarker;
pub use trade_tape::TradeRecord;
pub use rejected_intents::{RejectedIntentRecord, RejectionStats};
pub use execution_lab::{ExecutionPreset, PresetState};
