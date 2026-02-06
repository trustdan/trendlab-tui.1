//! TrendLab v3 TUI - Terminal interface for backtesting results
//!
//! Provides interactive exploration of strategy results with:
//! - Leaderboard rankings
//! - Equity curves with ghost overlay (execution drag)
//! - Trade tape drill-down
//! - Rejected intents timeline (why strategies stopped trading)
//! - Execution sensitivity analysis

pub mod app;
pub mod backtest_service;
pub mod theme;
pub mod navigation;
pub mod drill_down;
pub mod ghost_curve;
pub mod panels;
pub mod data_loader;
pub mod sample_data;

pub use app::App;
pub use theme::Theme;
pub use navigation::handle_key_event;

#[cfg(test)]
mod test_helpers;
