//! Ghost curve visualization (ideal vs real equity)
//!
//! Shows the execution drag between ideal fills (no slippage/spread)
//! and actual fills (with realistic execution costs).

mod ideal_equity;
mod real_equity;
mod drag_metric;

pub use ideal_equity::IdealEquity;
pub use real_equity::RealEquity;
pub use drag_metric::{DragMetric, GhostCurve};
