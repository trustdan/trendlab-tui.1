//! Drill-down navigation and state management
//!
//! Provides:
//! - DrillDownState enum (state machine for navigation)
//! - Summary card overlay
//! - Diagnostics views

mod flow;
mod summary_card;
mod diagnostics;

pub use flow::DrillDownState;
pub use summary_card::SummaryCard;
pub use diagnostics::Diagnostics;
pub use summary_card::SummaryCardData;
pub use diagnostics::DiagnosticData;
