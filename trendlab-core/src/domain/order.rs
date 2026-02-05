use crate::domain::ids::OrderId;
use serde::{Deserialize, Serialize};

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    StopMarket { stop_price: f64 },
    Limit { limit_price: f64 },
    StopLimit { stop_price: f64, limit_price: f64 },
}

/// Order state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    Pending,
    Triggered,
    Filled,
    Cancelled,
    Expired,
}

/// Order (minimal stub for M1, full implementation in M4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub state: OrderState,
}

impl Order {
    pub fn market(id: OrderId, symbol: String, side: OrderSide, quantity: f64) -> Self {
        Self {
            id,
            symbol,
            side,
            order_type: OrderType::Market,
            quantity,
            state: OrderState::Pending,
        }
    }
}
