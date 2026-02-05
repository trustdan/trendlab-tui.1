use crate::domain::ids::TradeId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Closed trade record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: TradeId,
    pub symbol: String,
    pub entry_time: DateTime<Utc>,
    pub entry_price: f64,
    pub exit_time: DateTime<Utc>,
    pub exit_price: f64,
    pub quantity: f64,
    pub pnl: f64,
    pub commission: f64,
}
