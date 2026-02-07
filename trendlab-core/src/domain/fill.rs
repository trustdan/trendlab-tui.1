//! Fill — a completed order execution.

use super::ids::OrderId;
use super::instrument::OrderSide;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Record of an order being filled (fully or partially).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: OrderId,
    pub bar_index: usize,
    pub date: NaiveDate,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
    pub commission: f64,
    pub slippage: f64,
}

impl Fill {
    /// Net cost for a buy fill, or net proceeds for a sell fill.
    pub fn net_amount(&self) -> f64 {
        let gross = self.price * self.quantity;
        match self.side {
            OrderSide::Buy => gross + self.commission + self.slippage,
            OrderSide::Sell => gross - self.commission - self.slippage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_net_amount_buy() {
        let fill = Fill {
            order_id: OrderId(1),
            bar_index: 4,
            date: NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            symbol: "SPY".into(),
            side: OrderSide::Buy,
            price: 100.0,
            quantity: 50.0,
            commission: 5.0,
            slippage: 2.0,
        };
        // Buy: cost = 100*50 + 5 + 2 = 5007
        assert_eq!(fill.net_amount(), 5007.0);
    }

    #[test]
    fn fill_net_amount_sell() {
        let fill = Fill {
            order_id: OrderId(2),
            bar_index: 8,
            date: NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(),
            symbol: "SPY".into(),
            side: OrderSide::Sell,
            price: 110.0,
            quantity: 50.0,
            commission: 5.0,
            slippage: 2.0,
        };
        // Sell: proceeds = 110*50 - 5 - 2 = 5493
        assert_eq!(fill.net_amount(), 5493.0);
    }
}
