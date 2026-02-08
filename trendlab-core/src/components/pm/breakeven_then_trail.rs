//! Breakeven then trail — two-phase position manager.
//!
//! Phase 1: Wait until unrealized profit reaches `breakeven_trigger_pct`.
//!          Once reached, move stop to entry price (breakeven).
//! Phase 2: Trail the stop at `trail_pct` below the highest high since entry.
//!
//! Phase detection: if `position.current_stop >= entry_price`, breakeven has
//! been reached. No extra state needed in the Position struct.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Breakeven-then-trail position manager.
#[derive(Debug, Clone)]
pub struct BreakevenThenTrail {
    /// Profit threshold to trigger breakeven (e.g., 0.05 for 5%).
    pub breakeven_trigger_pct: f64,
    /// Trail distance after breakeven (e.g., 0.10 for 10%).
    pub trail_pct: f64,
}

impl BreakevenThenTrail {
    pub fn new(breakeven_trigger_pct: f64, trail_pct: f64) -> Self {
        assert!(
            breakeven_trigger_pct > 0.0,
            "breakeven_trigger_pct must be positive"
        );
        assert!(trail_pct > 0.0, "trail_pct must be positive");
        assert!(trail_pct < 1.0, "trail_pct must be < 1.0");
        Self {
            breakeven_trigger_pct,
            trail_pct,
        }
    }
}

impl PositionManager for BreakevenThenTrail {
    fn name(&self) -> &str {
        "breakeven_then_trail"
    }

    fn on_bar(
        &self,
        position: &Position,
        _bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        let entry = position.avg_entry_price;

        match position.side {
            PositionSide::Long => {
                let breakeven_reached = position.current_stop.is_some_and(|s| s >= entry - 1e-10);

                if breakeven_reached {
                    // Phase 2: trailing from highest high
                    let stop = position.highest_price_since_entry * (1.0 - self.trail_pct);
                    OrderIntent::adjust_stop(stop)
                } else {
                    // Phase 1: waiting for breakeven trigger
                    let profit_pct = (position.highest_price_since_entry - entry) / entry;
                    if profit_pct >= self.breakeven_trigger_pct {
                        // Move to breakeven
                        OrderIntent::adjust_stop(entry)
                    } else {
                        OrderIntent::hold()
                    }
                }
            }
            PositionSide::Short => {
                let breakeven_reached = position.current_stop.is_some_and(|s| s <= entry + 1e-10);

                if breakeven_reached {
                    // Phase 2: trailing from lowest low
                    let stop = position.lowest_price_since_entry * (1.0 + self.trail_pct);
                    OrderIntent::adjust_stop(stop)
                } else {
                    // Phase 1: waiting for breakeven trigger
                    let profit_pct = (entry - position.lowest_price_since_entry) / entry;
                    if profit_pct >= self.breakeven_trigger_pct {
                        OrderIntent::adjust_stop(entry)
                    } else {
                        OrderIntent::hold()
                    }
                }
            }
            PositionSide::Flat => OrderIntent::hold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_bar(close: f64) -> Bar {
        Bar {
            symbol: "SPY".to_string(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: close - 0.5,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 1000,
            adj_close: close,
        }
    }

    #[test]
    fn long_phase1_holds_below_trigger() {
        let pm = BreakevenThenTrail::new(0.05, 0.10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 103.0; // 3% profit, below 5% trigger
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &make_bar(103.0), 1, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn long_phase1_triggers_breakeven() {
        let pm = BreakevenThenTrail::new(0.05, 0.10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 106.0; // 6% > 5% trigger
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &make_bar(106.0), 1, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::AdjustStop);
        assert_eq!(intent.stop_price, Some(100.0)); // breakeven = entry
    }

    #[test]
    fn long_phase2_trails() {
        let pm = BreakevenThenTrail::new(0.05, 0.10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 120.0;
        pos.current_stop = Some(100.0); // breakeven already reached
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &make_bar(118.0), 10, MarketStatus::Open, &iv);
        // stop = 120 * 0.9 = 108
        assert_eq!(intent.stop_price, Some(108.0));
    }

    #[test]
    fn short_phase1_triggers_breakeven() {
        let pm = BreakevenThenTrail::new(0.05, 0.10);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.lowest_price_since_entry = 94.0; // 6% profit for short
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &make_bar(94.0), 1, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(100.0)); // breakeven = entry
    }

    #[test]
    fn short_phase2_trails() {
        let pm = BreakevenThenTrail::new(0.05, 0.10);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.lowest_price_since_entry = 80.0;
        pos.current_stop = Some(100.0); // breakeven reached
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &make_bar(82.0), 10, MarketStatus::Open, &iv);
        // stop = 80 * 1.1 = 88
        assert_eq!(intent.stop_price, Some(88.0));
    }

    #[test]
    fn phase_transition_flow() {
        let pm = BreakevenThenTrail::new(0.05, 0.10);
        let iv = IndicatorValues::new();

        // Start: no profit yet
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 100.0;
        let intent = pm.on_bar(&pos, &make_bar(100.0), 0, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);

        // Price rises to trigger breakeven
        pos.highest_price_since_entry = 106.0;
        let intent = pm.on_bar(&pos, &make_bar(106.0), 1, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(100.0)); // breakeven

        // Simulate engine setting stop
        pos.current_stop = Some(100.0);

        // Now in trailing mode
        pos.highest_price_since_entry = 115.0;
        let intent = pm.on_bar(&pos, &make_bar(112.0), 5, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(103.5)); // 115 * 0.9
    }
}
