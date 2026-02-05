use crate::domain::ids::{FillId, OrderId};
use crate::domain::order::OrderSide;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Fill record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub id: FillId,
    pub order_id: OrderId,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
    pub commission: f64,
}
