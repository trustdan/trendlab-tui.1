//! Minimal smoke test engine
use crate::domain::Bar;

/// Simple trade record for smoke test (temporary, will use domain::Trade in M3)
#[derive(Debug, Clone)]
pub struct SmokeTrade {
    pub entry_bar: usize,
    pub entry_price: f64,
    pub exit_bar: usize,
    pub exit_price: f64,
    pub pnl: f64,
}

/// Minimal engine for M0.5 smoke test only
/// Will be replaced by real engine in M3
pub struct SmokeEngine {
    cash: f64,
    equity: f64,
    position_size: f64,
    entry_price: f64,
    entry_bar: usize,
    trades: Vec<SmokeTrade>,
}

impl SmokeEngine {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            cash: initial_cash,
            equity: initial_cash,
            position_size: 0.0,
            entry_price: 0.0,
            entry_bar: 0,
            trades: Vec::new(),
        }
    }

    /// Hardcoded buy (for smoke test only)
    pub fn execute_buy(&mut self, bar: &Bar, bar_index: usize, notional: f64) {
        let shares = notional / bar.close;
        self.position_size = shares;
        self.entry_price = bar.close;
        self.entry_bar = bar_index;
        self.cash -= notional;
        self.equity = self.cash + (self.position_size * bar.close);
    }

    /// Hardcoded sell (for smoke test only)
    pub fn execute_sell(&mut self, bar: &Bar, bar_index: usize) {
        let exit_value = self.position_size * bar.close;
        let pnl = exit_value - (self.position_size * self.entry_price);

        self.trades.push(SmokeTrade {
            entry_bar: self.entry_bar,
            entry_price: self.entry_price,
            exit_bar: bar_index,
            exit_price: bar.close,
            pnl,
        });

        self.cash += exit_value;
        self.position_size = 0.0;
        self.equity = self.cash;
    }

    /// Mark to market (update equity)
    pub fn mark_to_market(&mut self, bar: &Bar) {
        self.equity = self.cash + (self.position_size * bar.close);
    }

    pub fn equity(&self) -> f64 {
        self.equity
    }

    pub fn trades(&self) -> &[SmokeTrade] {
        &self.trades
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_smoke_engine_buy_sell() {
        let mut engine = SmokeEngine::new(10000.0);
        let bar_buy = Bar::new(Utc::now(), "TEST".into(), 107.0, 112.0, 106.0, 110.0, 1000.0);
        let bar_sell = Bar::new(Utc::now(), "TEST".into(), 118.0, 125.0, 117.0, 120.0, 1000.0);

        engine.execute_buy(&bar_buy, 3, 100.0);
        assert!(engine.position_size > 0.0);

        engine.execute_sell(&bar_sell, 7);
        assert_eq!(engine.position_size, 0.0);
        assert!(engine.equity() > 10000.0);
    }
}
