//! Backtesting engine — bar-by-bar event loop and supporting infrastructure.
//!
//! The engine consumes aligned bar data (from the data pipeline) and indicator
//! values (precomputed), then runs the four-phase bar loop:
//!
//! 1. Start-of-bar: activate day orders, fill MOO orders
//! 2. Intrabar: simulate trigger checks for stop/limit orders
//! 3. End-of-bar: fill MOC orders
//! 4. Post-bar: mark-to-market, equity accounting, PM maintenance orders

pub mod convert;
pub mod execution;
pub mod loop_runner;
pub mod order_book;
pub mod portfolio_update;
pub mod precompute;
pub mod state;
pub mod stickiness;
pub mod trade_extraction;

pub use convert::{aligned_to_bars, raw_to_bar};
pub use execution::{
    CostModel, ExecutionConfig, ExecutionEngine, LiquidityPolicy, RemainderPolicy,
};
pub use loop_runner::run_backtest;
pub use order_book::{OrderBook, OrderBookError};
pub use portfolio_update::apply_fills;
pub use precompute::{compute_warmup, precompute_indicators};
pub use state::{EngineConfig, EngineState, RunResult};
