//! Execution engine: converts triggered orders into fills with realistic simulation
//!
//! Key concepts:
//! - **Fill phases**: SOB → Intrabar → EOB
//! - **Path policies**: How to resolve intrabar ambiguity
//! - **Gap rules**: Stops that gap through fill at worse price
//! - **Order priority**: Which order fills first in ambiguous bars
//! - **Slippage**: Cost added to fill price
//! - **Liquidity**: Optional participation limits

pub mod fill_engine;
pub mod gap_handler;
pub mod liquidity;
pub mod path_policy;
pub mod preset;
pub mod priority;
pub mod slippage;

pub use fill_engine::{FillEngine, FillResult};
pub use gap_handler::GapHandler;
pub use liquidity::{LiquidityConstraint, RemainderPolicy};
pub use path_policy::{BestCase, Deterministic, PathPolicy, PriceOrder, WorstCase};
pub use preset::{ExecutionPreset, Hostile, Optimistic, Realistic};
pub use priority::{BestCasePriority, PriorityPolicy, WorstCasePriority};
pub use slippage::{AtrSlippage, FixedSlippage, SlippageModel};
