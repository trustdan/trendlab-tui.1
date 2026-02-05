use crate::domain::{Bar, Portfolio};
use crate::engine::{EquityTracker, WarmupState};
use std::collections::HashMap;

/// Main backtest engine
pub struct Engine {
    warmup: WarmupState,
    accounting: EquityTracker,
    portfolio: Portfolio,
    current_bar_index: usize,
}

impl Engine {
    pub fn new(initial_cash: f64, warmup_bars: usize) -> Self {
        Self {
            warmup: WarmupState::new(warmup_bars),
            accounting: EquityTracker::new(initial_cash),
            portfolio: Portfolio::new(initial_cash),
            current_bar_index: 0,
        }
    }

    /// Process a single bar (4-phase event loop)
    pub fn process_bar(&mut self, bar: &Bar, current_prices: &HashMap<String, f64>) {
        // Phase 1: Start-of-bar
        self.start_of_bar(bar);

        // Phase 2: Intrabar (simulated in M5)
        self.intrabar(bar);

        // Phase 3: End-of-bar
        self.end_of_bar(bar);

        // Phase 4: Post-bar
        self.post_bar(bar, current_prices);

        self.current_bar_index += 1;
        self.warmup.process_bar();
    }

    fn start_of_bar(&mut self, _bar: &Bar) {
        // Activate day orders (M4)
        // Fill MOO orders (M5)
    }

    fn intrabar(&mut self, _bar: &Bar) {
        // Simulate triggers/fills using PathPolicy (M5)
    }

    fn end_of_bar(&mut self, _bar: &Bar) {
        // Fill MOC orders (M5)
    }

    fn post_bar(&mut self, _bar: &Bar, current_prices: &HashMap<String, f64>) {
        // Mark to market
        let equity = self.accounting.compute_equity(&self.portfolio.positions, current_prices);
        self.accounting.record_equity(equity);

        // PM emits maintenance orders for NEXT bar (M6)
        // Only if warmup complete
        if self.warmup.is_warm() {
            // PM logic goes here in M6
        }
    }

    pub fn is_warm(&self) -> bool {
        self.warmup.is_warm()
    }

    pub fn equity_history(&self) -> &[f64] {
        self.accounting.equity_history()
    }

    pub fn current_bar_index(&self) -> usize {
        self.current_bar_index
    }

    pub fn cash(&self) -> f64 {
        self.accounting.cash()
    }

    pub fn portfolio(&self) -> &Portfolio {
        &self.portfolio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Bar;
    use chrono::Utc;

    #[test]
    fn test_warmup_blocks_pm() {
        let mut engine = Engine::new(10000.0, 20);
        let bars: Vec<_> = (0..50)
            .map(|i| Bar {
                timestamp: Utc::now(),
                symbol: "SPY".into(),
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 102.0,
                volume: 1000.0 + i as f64,
            })
            .collect();

        // Process first 19 bars - should not be warm
        for bar in &bars[0..19] {
            let mut prices = HashMap::new();
            prices.insert("SPY".to_string(), bar.close);
            engine.process_bar(bar, &prices);
        }
        assert!(!engine.is_warm());

        // Process 20th bar - should now be warm
        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), bars[19].close);
        engine.process_bar(&bars[19], &prices);
        assert!(engine.is_warm());
    }

    #[test]
    fn test_equity_tracking_per_bar() {
        let mut engine = Engine::new(10000.0, 0); // no warmup

        let bars = vec![
            Bar {
                timestamp: Utc::now(),
                symbol: "SPY".into(),
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 102.0,
                volume: 1000.0,
            },
            Bar {
                timestamp: Utc::now(),
                symbol: "SPY".into(),
                open: 102.0,
                high: 108.0,
                low: 100.0,
                close: 105.0,
                volume: 1100.0,
            },
        ];

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), bars[0].close);
        engine.process_bar(&bars[0], &prices);

        prices.insert("SPY".to_string(), bars[1].close);
        engine.process_bar(&bars[1], &prices);

        // Should have 3 equity points: initial + 2 bars
        assert_eq!(engine.equity_history().len(), 3);
        assert_eq!(engine.equity_history()[0], 10000.0);
    }

    #[test]
    fn test_bar_index_tracking() {
        let mut engine = Engine::new(10000.0, 0);
        assert_eq!(engine.current_bar_index(), 0);

        let bar = Bar {
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 102.0,
            volume: 1000.0,
        };

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), bar.close);

        engine.process_bar(&bar, &prices);
        assert_eq!(engine.current_bar_index(), 1);

        engine.process_bar(&bar, &prices);
        assert_eq!(engine.current_bar_index(), 2);
    }

    #[test]
    fn test_four_phase_loop_executes() {
        let mut engine = Engine::new(10000.0, 0);

        let bar = Bar {
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 102.0,
            volume: 1000.0,
        };

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), bar.close);

        // Should not panic and should complete all 4 phases
        engine.process_bar(&bar, &prices);

        // Verify state updated
        assert_eq!(engine.current_bar_index(), 1);
        assert_eq!(engine.equity_history().len(), 2); // initial + 1 bar
    }
}
