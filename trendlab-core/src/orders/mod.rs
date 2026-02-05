// ! Order management system
//!
//! Provides order types, order lifecycle state machine, brackets/OCO, and order policies.

pub mod bracket;
pub mod order;
pub mod order_book;
pub mod order_policy;
pub mod order_type;

pub use bracket::BracketOrderBuilder;
pub use order::{Order, OrderState};
pub use order_book::{OrderBook, OrderBookError};
pub use order_policy::{BreakoutPolicy, ImmediatePolicy, MeanReversionPolicy, OrderPolicy};
pub use order_type::{MarketTiming, OrderType, StopDirection};
