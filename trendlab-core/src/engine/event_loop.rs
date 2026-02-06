use crate::domain::{Bar, Portfolio};
use crate::engine::{EquityTracker, WarmupState};
use crate::order_policy::guards::{Guard, RejectedIntent};
use std::collections::HashMap;

/// Main backtest engine
pub struct Engine {
    warmup: WarmupState,
    accounting: EquityTracker,
    portfolio: Portfolio,
    current_bar_index: usize,
    /// Intrabar path policy name (e.g. "WorstCase", "BestCase", "OhlcOrder")
    intrabar_policy: String,
    /// Rejection guards evaluated per bar
    guards: Vec<Box<dyn Guard>>,
    /// Accumulated rejected intents across the run
    rejected_intents: Vec<RejectedIntent>,
}

impl Engine {
    pub fn new(initial_cash: f64, warmup_bars: usize) -> Self {
        Self {
            warmup: WarmupState::new(warmup_bars),
            accounting: EquityTracker::new(initial_cash),
            portfolio: Portfolio::new(initial_cash),
            current_bar_index: 0,
            intrabar_policy: "WorstCase".to_string(),
            guards: Vec::new(),
            rejected_intents: Vec::new(),
        }
    }

    /// Create an engine with rejection guards.
    pub fn with_guards(
        initial_cash: f64,
        warmup_bars: usize,
        guards: Vec<Box<dyn Guard>>,
    ) -> Self {
        Self {
            warmup: WarmupState::new(warmup_bars),
            accounting: EquityTracker::new(initial_cash),
            portfolio: Portfolio::new(initial_cash),
            current_bar_index: 0,
            intrabar_policy: "WorstCase".to_string(),
            guards,
            rejected_intents: Vec::new(),
        }
    }

    /// Create an engine with a specific intrabar path policy.
    pub fn with_policy(
        initial_cash: f64,
        warmup_bars: usize,
        policy: &str,
    ) -> Self {
        Self {
            warmup: WarmupState::new(warmup_bars),
            accounting: EquityTracker::new(initial_cash),
            portfolio: Portfolio::new(initial_cash),
            current_bar_index: 0,
            intrabar_policy: policy.to_string(),
            guards: Vec::new(),
            rejected_intents: Vec::new(),
        }
    }

    /// Create an engine with both guards and a path policy.
    pub fn with_guards_and_policy(
        initial_cash: f64,
        warmup_bars: usize,
        guards: Vec<Box<dyn Guard>>,
        policy: &str,
    ) -> Self {
        Self {
            warmup: WarmupState::new(warmup_bars),
            accounting: EquityTracker::new(initial_cash),
            portfolio: Portfolio::new(initial_cash),
            current_bar_index: 0,
            intrabar_policy: policy.to_string(),
            guards,
            rejected_intents: Vec::new(),
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
        // The intrabar_policy field selects the fill ordering:
        // - "WorstCase": adverse fill ordering (stop-loss before take-profit)
        // - "BestCase": favorable fill ordering
        // - "OhlcOrder": follow O→H→L→C or O→L→H→C based on bar direction
        let _ = &self.intrabar_policy;
    }

    fn end_of_bar(&mut self, _bar: &Bar) {
        // Fill MOC orders (M5)
    }

    fn post_bar(&mut self, bar: &Bar, current_prices: &HashMap<String, f64>) {
        // Mark to market
        let equity = self.accounting.compute_equity(&self.portfolio.positions, current_prices);
        self.accounting.record_equity(equity);

        // Evaluate rejection guards (only after warmup)
        if self.warmup.is_warm() {
            let cash = self.accounting.cash();
            let open_positions = self.portfolio.positions.len();

            for guard in &self.guards {
                if let Some(rejection) = guard.evaluate(
                    bar,
                    self.current_bar_index,
                    cash,
                    open_positions,
                ) {
                    self.rejected_intents.push(rejection);
                }
            }

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

    /// Get all rejected intents accumulated during the run.
    pub fn rejected_intents(&self) -> &[RejectedIntent] {
        &self.rejected_intents
    }

    /// Get the intrabar policy name.
    pub fn intrabar_policy(&self) -> &str {
        &self.intrabar_policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Bar;
    use crate::order_policy::guards::{default_guards, RejectionReason};
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

    #[test]
    fn test_guards_produce_rejections() {
        let guards = default_guards();
        let mut engine = Engine::with_guards(10000.0, 0, guards);

        // Bar with very high volatility (range = 20/100 = 0.20 > 0.05 threshold)
        // but normal volume (above 100k min)
        let bar = Bar {
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 110.0,
            low: 90.0,
            close: 100.0,
            volume: 1_000_000.0,
        };

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), bar.close);
        engine.process_bar(&bar, &prices);

        // VolatilityGuard should fire (range 0.20 > 0.05)
        let rejections = engine.rejected_intents();
        assert!(!rejections.is_empty());
        assert!(rejections.iter().any(|r| r.reason == RejectionReason::VolatilityGuard));
    }

    #[test]
    fn test_no_rejections_during_warmup() {
        let guards = default_guards();
        let mut engine = Engine::with_guards(10000.0, 5, guards);

        // Volatile bar during warmup
        let bar = Bar {
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            open: 100.0,
            high: 120.0,
            low: 80.0,
            close: 100.0,
            volume: 1_000_000.0,
        };

        let mut prices = HashMap::new();
        prices.insert("SPY".to_string(), bar.close);

        // Process during warmup (bars 0-4)
        for _ in 0..3 {
            engine.process_bar(&bar, &prices);
        }

        // No rejections during warmup
        assert!(engine.rejected_intents().is_empty());
    }

    #[test]
    fn test_with_policy_constructor() {
        let engine = Engine::with_policy(10000.0, 0, "BestCase");
        assert_eq!(engine.intrabar_policy(), "BestCase");
    }
}
