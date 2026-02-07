//! Component traits — the four-component composition model.
//!
//! Every strategy is composed of exactly four independent components:
//! - Signal generator: detects market events, emits directional intent
//! - Signal filter: gates entry signals based on market conditions
//! - Execution model: determines order type and fill parameters
//! - Position manager: manages open positions, emits exit/adjustment intents
//!
//! Plus the indicator trait for precomputed numeric series.

pub mod execution;
pub mod filter;
pub mod indicator;
pub mod pm;
pub mod signal;

pub use execution::{ExecutionModel, ExecutionPreset, GapPolicy, PathPolicy};
pub use filter::SignalFilter;
pub use indicator::{Indicator, IndicatorValues};
pub use pm::{IntentAction, OrderIntent, PositionManager};
pub use signal::{FilterVerdict, SignalDirection, SignalEvaluation, SignalEvent, SignalGenerator};
