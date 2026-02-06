//! Drill-down state machine
//!
//! Defines the navigation states and transitions for exploring strategy results.

/// Drill-down navigation states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrillDownState {
    /// Main leaderboard view (top-level)
    Leaderboard,

    /// Summary card overlay showing strategy stats
    SummaryCard(String), // run_id

    /// Trade tape (list of all trades)
    TradeTape(String), // run_id

    /// Rejected intents timeline (blocked signals)
    RejectedIntents(String), // run_id

    /// Chart view focused on specific trade
    ChartWithTrade(String, String), // run_id, trade_id

    /// Diagnostics for specific trade (slippage, gaps)
    Diagnostics(String, String), // run_id, trade_id

    /// Execution lab (rerun with different execution)
    ExecutionLab(String), // run_id

    /// Sensitivity analysis (cross-preset comparison)
    Sensitivity(String), // run_id

    /// Run manifest viewer (full config display)
    RunManifest(String), // run_id

    /// Robustness ladder visualization
    Robustness(String), // run_id
}

impl DrillDownState {
    /// Get the run ID associated with this state (if any)
    pub fn run_id(&self) -> Option<&str> {
        match self {
            DrillDownState::Leaderboard => None,
            DrillDownState::SummaryCard(run_id)
            | DrillDownState::TradeTape(run_id)
            | DrillDownState::RejectedIntents(run_id)
            | DrillDownState::ChartWithTrade(run_id, _)
            | DrillDownState::Diagnostics(run_id, _)
            | DrillDownState::ExecutionLab(run_id)
            | DrillDownState::Sensitivity(run_id)
            | DrillDownState::RunManifest(run_id)
            | DrillDownState::Robustness(run_id) => Some(run_id),
        }
    }

    /// Get the trade ID associated with this state (if any)
    pub fn trade_id(&self) -> Option<&str> {
        match self {
            DrillDownState::ChartWithTrade(_, trade_id)
            | DrillDownState::Diagnostics(_, trade_id) => Some(trade_id),
            _ => None,
        }
    }

    /// Check if this state is at the top level (leaderboard)
    pub fn is_top_level(&self) -> bool {
        matches!(self, DrillDownState::Leaderboard)
    }

    /// Get human-readable description of current state
    pub fn description(&self) -> &'static str {
        match self {
            DrillDownState::Leaderboard => "Leaderboard",
            DrillDownState::SummaryCard(_) => "Strategy Summary",
            DrillDownState::TradeTape(_) => "Trade Tape",
            DrillDownState::RejectedIntents(_) => "Rejected Intents",
            DrillDownState::ChartWithTrade(_, _) => "Trade Chart",
            DrillDownState::Diagnostics(_, _) => "Trade Diagnostics",
            DrillDownState::ExecutionLab(_) => "Execution Lab",
            DrillDownState::Sensitivity(_) => "Sensitivity Analysis",
            DrillDownState::RunManifest(_) => "Run Manifest",
            DrillDownState::Robustness(_) => "Robustness Ladder",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drill_down_state_run_id() {
        let state = DrillDownState::SummaryCard("run_123".to_string());
        assert_eq!(state.run_id(), Some("run_123"));

        let state = DrillDownState::Leaderboard;
        assert_eq!(state.run_id(), None);
    }

    #[test]
    fn test_drill_down_state_trade_id() {
        let state = DrillDownState::ChartWithTrade("run_1".to_string(), "trade_5".to_string());
        assert_eq!(state.trade_id(), Some("trade_5"));

        let state = DrillDownState::TradeTape("run_1".to_string());
        assert_eq!(state.trade_id(), None);
    }

    #[test]
    fn test_is_top_level() {
        assert!(DrillDownState::Leaderboard.is_top_level());
        assert!(!DrillDownState::SummaryCard("run_1".to_string()).is_top_level());
    }

    #[test]
    fn test_description() {
        assert_eq!(DrillDownState::Leaderboard.description(), "Leaderboard");
        assert_eq!(
            DrillDownState::SummaryCard("run_1".to_string()).description(),
            "Strategy Summary"
        );
        assert_eq!(
            DrillDownState::TradeTape("run_1".to_string()).description(),
            "Trade Tape"
        );
    }
}
