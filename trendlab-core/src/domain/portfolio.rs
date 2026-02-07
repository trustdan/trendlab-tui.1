//! Portfolio — aggregate state of cash + all open positions.

use super::position::Position;
use std::collections::HashMap;

/// Aggregate portfolio state.
///
/// Tracks cash, open positions, and accumulated costs. The equity accounting
/// identity must hold at every bar: `equity == cash + sum(position market values)`.
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub cash: f64,
    pub initial_capital: f64,
    pub positions: HashMap<String, Position>,
    pub total_commission: f64,
    pub total_slippage: f64,
}

impl Portfolio {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            cash: initial_capital,
            initial_capital,
            positions: HashMap::new(),
            total_commission: 0.0,
            total_slippage: 0.0,
        }
    }

    /// Total equity = cash + sum of all position market values.
    pub fn equity(&self, prices: &HashMap<String, f64>) -> f64 {
        let position_value: f64 = self
            .positions
            .iter()
            .map(|(sym, pos)| {
                let price = prices.get(sym).copied().unwrap_or(pos.avg_entry_price);
                pos.market_value(price)
            })
            .sum();
        self.cash + position_value
    }

    /// Whether a symbol has an open position.
    pub fn has_position(&self, symbol: &str) -> bool {
        self.positions.get(symbol).is_some_and(|p| !p.is_flat())
    }

    /// Get a position by symbol (if exists and not flat).
    pub fn get_position(&self, symbol: &str) -> Option<&Position> {
        self.positions.get(symbol).filter(|p| !p.is_flat())
    }

    /// Get a mutable position by symbol.
    pub fn get_position_mut(&mut self, symbol: &str) -> Option<&mut Position> {
        self.positions.get_mut(symbol).filter(|p| !p.is_flat())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equity_with_no_positions() {
        let portfolio = Portfolio::new(100_000.0);
        let prices = HashMap::new();
        assert_eq!(portfolio.equity(&prices), 100_000.0);
    }

    #[test]
    fn equity_with_position() {
        let mut portfolio = Portfolio::new(90_000.0);
        portfolio.positions.insert(
            "SPY".into(),
            Position::new_long("SPY".into(), 100.0, 100.0, 0),
        );
        let mut prices = HashMap::new();
        prices.insert("SPY".into(), 110.0);
        // 90_000 + 100 * 110 = 101_000
        assert_eq!(portfolio.equity(&prices), 101_000.0);
    }

    #[test]
    fn has_position_checks() {
        let mut portfolio = Portfolio::new(100_000.0);
        assert!(!portfolio.has_position("SPY"));
        portfolio.positions.insert(
            "SPY".into(),
            Position::new_long("SPY".into(), 100.0, 100.0, 0),
        );
        assert!(portfolio.has_position("SPY"));
    }
}
