//! Domain types — the vocabulary of TrendLab.
//!
//! Every module in the system builds on these types. They define bars, orders,
//! fills, positions, portfolios, trades, instruments, and deterministic IDs.

pub mod bar;
pub mod fill;
pub mod ids;
pub mod instrument;
pub mod order;
pub mod portfolio;
pub mod position;
pub mod trade;

// Re-export the most commonly used types at the domain level.
pub use bar::{Bar, MarketStatus};
pub use fill::Fill;
pub use ids::{
    ConfigHash, DatasetHash, FullHash, IdGen, OcoGroupId, OrderId, RunId, SignalEventId,
};
pub use instrument::{AssetClass, Instrument, OrderSide};
pub use order::{BracketOrder, OcoGroup, Order, OrderAuditEntry, OrderStatus, OrderType};
pub use portfolio::Portfolio;
pub use position::{Position, PositionSide};
pub use trade::TradeRecord;
