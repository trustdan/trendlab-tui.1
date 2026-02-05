use crate::domain::{Fill, OrderSide, Position};
use std::collections::HashMap;

/// Equity and PnL tracker
#[derive(Debug, Clone)]
pub struct EquityTracker {
    initial_cash: f64,
    cash: f64,
    realized_pnl: f64,
    commission_paid: f64,
    equity_history: Vec<f64>,
}

impl EquityTracker {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            initial_cash,
            cash: initial_cash,
            realized_pnl: 0.0,
            commission_paid: 0.0,
            equity_history: vec![initial_cash],
        }
    }

    /// Apply a fill to cash and realized PnL
    pub fn apply_fill(&mut self, fill: &Fill, avg_entry_price: f64) {
        // Update cash (inflow for sells, outflow for buys)
        match fill.side {
            OrderSide::Buy => {
                self.cash -= fill.price * fill.quantity;
            }
            OrderSide::Sell => {
                self.cash += fill.price * fill.quantity;
                // Realize PnL on sell
                let pnl = (fill.price - avg_entry_price) * fill.quantity;
                self.realized_pnl += pnl;
            }
        }

        // Deduct commission
        self.cash -= fill.commission;
        self.commission_paid += fill.commission;
    }

    /// Compute current equity (cash + position value)
    pub fn compute_equity(
        &self,
        positions: &HashMap<String, Position>,
        prices: &HashMap<String, f64>,
    ) -> f64 {
        let position_value: f64 = positions
            .iter()
            .map(|(symbol, pos)| {
                let price = prices.get(symbol).copied().unwrap_or(0.0);
                pos.market_value(price)
            })
            .sum();

        self.cash + position_value
    }

    /// Record equity at bar close
    pub fn record_equity(&mut self, equity: f64) {
        self.equity_history.push(equity);
    }

    /// Get unrealized PnL
    pub fn unrealized_pnl(
        &self,
        positions: &HashMap<String, Position>,
        prices: &HashMap<String, f64>,
    ) -> f64 {
        positions
            .iter()
            .map(|(symbol, pos)| {
                let price = prices.get(symbol).copied().unwrap_or(0.0);
                pos.unrealized_pnl(price)
            })
            .sum()
    }

    pub fn cash(&self) -> f64 {
        self.cash
    }

    pub fn realized_pnl(&self) -> f64 {
        self.realized_pnl
    }

    pub fn commission_paid(&self) -> f64 {
        self.commission_paid
    }

    pub fn total_pnl(&self, current_equity: f64) -> f64 {
        current_equity - self.initial_cash
    }

    pub fn equity_history(&self) -> &[f64] {
        &self.equity_history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FillId, OrderId};
    use chrono::Utc;

    #[test]
    fn test_equity_tracking() {
        let mut tracker = EquityTracker::new(10000.0);

        // Simulate buy fill
        let buy_fill = Fill {
            id: FillId::new("fill1".into()),
            order_id: OrderId::new("order1"),
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            side: OrderSide::Buy,
            price: 100.0,
            quantity: 10.0,
            commission: 1.0,
        };

        tracker.apply_fill(&buy_fill, 0.0);
        assert_eq!(tracker.cash(), 10000.0 - 1000.0 - 1.0); // cash - cost - commission

        // Simulate sell fill
        let sell_fill = Fill {
            id: FillId::new("fill2".into()),
            order_id: OrderId::new("order2"),
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            side: OrderSide::Sell,
            price: 110.0,
            quantity: 10.0,
            commission: 1.0,
        };

        tracker.apply_fill(&sell_fill, 100.0); // avg entry = 100
        assert_eq!(tracker.realized_pnl(), 100.0); // 10 shares * $10 profit
        assert_eq!(tracker.commission_paid(), 2.0);
    }

    #[test]
    fn test_compute_equity() {
        let tracker = EquityTracker::new(10000.0);

        let mut positions = HashMap::new();
        positions.insert(
            "SPY".to_string(),
            Position {
                symbol: "SPY".to_string(),
                quantity: 10.0,
                avg_entry_price: 100.0,
            },
        );

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), 110.0);

        let equity = tracker.compute_equity(&positions, &prices);
        assert_eq!(equity, 10000.0 + 10.0 * 110.0); // cash + position value
    }

    #[test]
    fn test_unrealized_pnl() {
        let tracker = EquityTracker::new(10000.0);

        let mut positions = HashMap::new();
        positions.insert(
            "SPY".to_string(),
            Position {
                symbol: "SPY".to_string(),
                quantity: 10.0,
                avg_entry_price: 100.0,
            },
        );

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), 110.0);

        let unrealized = tracker.unrealized_pnl(&positions, &prices);
        assert_eq!(unrealized, 10.0 * 10.0); // 10 shares * $10 profit per share
    }

    #[test]
    fn test_equity_history() {
        let mut tracker = EquityTracker::new(10000.0);
        assert_eq!(tracker.equity_history().len(), 1);
        assert_eq!(tracker.equity_history()[0], 10000.0);

        tracker.record_equity(10100.0);
        tracker.record_equity(10200.0);

        assert_eq!(tracker.equity_history().len(), 3);
        assert_eq!(tracker.equity_history()[1], 10100.0);
        assert_eq!(tracker.equity_history()[2], 10200.0);
    }

    #[test]
    fn test_total_pnl() {
        let tracker = EquityTracker::new(10000.0);
        assert_eq!(tracker.total_pnl(11000.0), 1000.0);
        assert_eq!(tracker.total_pnl(9500.0), -500.0);
    }

    #[test]
    fn test_commission_tracking() {
        let mut tracker = EquityTracker::new(10000.0);

        let fill1 = Fill {
            id: FillId::new("fill1".into()),
            order_id: OrderId::new("order1"),
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            side: OrderSide::Buy,
            price: 100.0,
            quantity: 10.0,
            commission: 1.5,
        };

        let fill2 = Fill {
            id: FillId::new("fill2".into()),
            order_id: OrderId::new("order2"),
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            side: OrderSide::Sell,
            price: 110.0,
            quantity: 10.0,
            commission: 1.5,
        };

        tracker.apply_fill(&fill1, 0.0);
        tracker.apply_fill(&fill2, 100.0);

        assert_eq!(tracker.commission_paid(), 3.0);
    }
}
