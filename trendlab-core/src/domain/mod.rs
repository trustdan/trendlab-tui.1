//! Domain types for TrendLab v3

pub mod bar;
pub mod fill;
pub mod ids;
pub mod instrument;
pub mod order;
pub mod portfolio;
pub mod position;
pub mod trade;

pub use bar::{Bar, BarError};
pub use fill::Fill;
pub use ids::{ConfigId, DatasetHash, FillId, OrderId, RunId, TradeId};
pub use instrument::{AssetClass, Instrument, InstrumentError, OrderSideForRounding, TickPolicy};
pub use order::{Order, OrderSide, OrderState, OrderType};
pub use portfolio::Portfolio;
pub use position::Position;
pub use trade::Trade;

/// Symbol type alias
pub type Symbol = String;
