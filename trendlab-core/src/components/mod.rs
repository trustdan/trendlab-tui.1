//! Component traits — the four-component composition model.
//!
//! Every strategy is composed of exactly four independent components:
//! - Signal generator: detects market events, emits directional intent
//! - Signal filter: gates entry signals based on market conditions
//! - Execution model: determines order type and fill parameters
//! - Position manager: manages open positions, emits exit/adjustment intents
//!
//! Plus the indicator trait for precomputed numeric series.

pub mod composition;
pub mod execution;
pub mod factory;
pub mod filter;
pub mod indicator;
pub mod pm;
pub mod sampler;
pub mod signal;

pub use composition::{
    build_composition, check_compatibility, CompatibilityResult, StrategyComposition,
    StrategyPreset,
};
pub use execution::{
    CloseOnSignalModel, ExecutionModel, ExecutionPreset, GapPolicy, LimitEntryModel,
    NextBarOpenModel, PathPolicy, StopEntryModel,
};
pub use factory::{
    create_execution, create_filter, create_pm, create_signal, required_indicators, FactoryError,
};
pub use filter::SignalFilter;
pub use indicator::{Indicator, IndicatorValues};
pub use pm::{
    AtrTrailing, BreakevenThenTrail, Chandelier, FixedStopLoss, FrozenReference, IntentAction,
    MaxHoldingPeriod, NoOpPm, OrderIntent, PercentTrailing, PositionManager, SinceEntryTrailing,
    TimeDecay,
};
pub use sampler::{sample_composition, ComponentPool, ComponentVariant, ParamRange};
pub use signal::{FilterVerdict, SignalDirection, SignalEvaluation, SignalEvent, SignalGenerator};
