use crate::domain::position::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Portfolio accounting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub cash: f64,
    pub positions: HashMap<String, Position>,
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Self { cash: initial_cash, positions: HashMap::new() }
    }

    pub fn equity(&self, current_prices: &HashMap<String, f64>) -> f64 {
        let position_value: f64 = self
            .positions
            .iter()
            .map(|(symbol, pos)| {
                let price = current_prices.get(symbol).copied().unwrap_or(0.0);
                pos.market_value(price)
            })
            .sum();

        self.cash + position_value
    }
}
