//! Position — an open holding in a single symbol.

use serde::{Deserialize, Serialize};

/// Direction of a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
    Flat,
}

/// An open position in a single symbol.
///
/// Tracks entry details and running statistics needed by position managers
/// (highest/lowest since entry, bars held, unrealized PnL).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub side: PositionSide,
    pub quantity: f64,
    pub avg_entry_price: f64,
    pub entry_bar: usize,
    /// Highest price observed since position was opened (for trailing stops).
    pub highest_price_since_entry: f64,
    /// Lowest price observed since position was opened (for short trailing stops).
    pub lowest_price_since_entry: f64,
    /// Number of bars the position has been held (incremented each bar including void bars).
    pub bars_held: usize,
    /// Current unrealized PnL based on last mark-to-market price.
    pub unrealized_pnl: f64,
    /// Accumulated realized PnL from partial exits.
    pub realized_pnl: f64,
    /// Current stop price (set by position manager, used by ratchet invariant check).
    pub current_stop: Option<f64>,
}

impl Position {
    pub fn new_long(symbol: String, quantity: f64, entry_price: f64, entry_bar: usize) -> Self {
        Self {
            symbol,
            side: PositionSide::Long,
            quantity,
            avg_entry_price: entry_price,
            entry_bar,
            highest_price_since_entry: entry_price,
            lowest_price_since_entry: entry_price,
            bars_held: 0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            current_stop: None,
        }
    }

    pub fn new_short(symbol: String, quantity: f64, entry_price: f64, entry_bar: usize) -> Self {
        Self {
            symbol,
            side: PositionSide::Short,
            quantity,
            avg_entry_price: entry_price,
            entry_bar,
            highest_price_since_entry: entry_price,
            lowest_price_since_entry: entry_price,
            bars_held: 0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            current_stop: None,
        }
    }

    pub fn is_flat(&self) -> bool {
        self.side == PositionSide::Flat || self.quantity == 0.0
    }

    /// Update running statistics with a new bar's price data.
    pub fn update_mark(&mut self, current_price: f64) {
        if current_price > self.highest_price_since_entry {
            self.highest_price_since_entry = current_price;
        }
        if current_price < self.lowest_price_since_entry {
            self.lowest_price_since_entry = current_price;
        }
        self.unrealized_pnl = match self.side {
            PositionSide::Long => (current_price - self.avg_entry_price) * self.quantity,
            PositionSide::Short => (self.avg_entry_price - current_price) * self.quantity,
            PositionSide::Flat => 0.0,
        };
    }

    /// Increment the bars-held counter (called every bar, including void bars).
    pub fn tick_bar(&mut self) {
        self.bars_held += 1;
    }

    /// Market value at the given price.
    pub fn market_value(&self, price: f64) -> f64 {
        self.quantity * price
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_position_unrealized_pnl() {
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.update_mark(110.0);
        assert_eq!(pos.unrealized_pnl, 1000.0);
    }

    #[test]
    fn short_position_unrealized_pnl() {
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.update_mark(90.0);
        assert_eq!(pos.unrealized_pnl, 1000.0);
    }

    #[test]
    fn highest_lowest_tracking() {
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.update_mark(110.0);
        pos.update_mark(95.0);
        pos.update_mark(105.0);
        assert_eq!(pos.highest_price_since_entry, 110.0);
        assert_eq!(pos.lowest_price_since_entry, 95.0);
    }

    #[test]
    fn bars_held_increments() {
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.tick_bar();
        pos.tick_bar();
        pos.tick_bar();
        assert_eq!(pos.bars_held, 3);
    }
}
