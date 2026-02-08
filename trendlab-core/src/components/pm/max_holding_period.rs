//! Max holding period — force exit after N bars regardless of price.
//!
//! A pure time-based exit. On void bars, `bars_held` still increments
//! (via `Position::tick_bar()`), so void bars count toward the limit.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position};

use super::{OrderIntent, PositionManager};

/// Max holding period position manager.
#[derive(Debug, Clone)]
pub struct MaxHoldingPeriod {
    /// Maximum number of bars to hold before force exit.
    pub max_bars: usize,
}

impl MaxHoldingPeriod {
    pub fn new(max_bars: usize) -> Self {
        assert!(max_bars > 0, "max_bars must be > 0");
        Self { max_bars }
    }
}

impl PositionManager for MaxHoldingPeriod {
    fn name(&self) -> &str {
        "max_holding_period"
    }

    fn on_bar(
        &self,
        position: &Position,
        _bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        if position.bars_held >= self.max_bars {
            OrderIntent::force_exit()
        } else {
            OrderIntent::hold()
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
    fn holds_before_max() {
        let pm = MaxHoldingPeriod::new(10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 9;
        let bar = make_bar(110.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 9, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn exits_at_max() {
        let pm = MaxHoldingPeriod::new(10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 10;
        let bar = make_bar(110.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 10, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::ForceExit);
    }

    #[test]
    fn exits_past_max() {
        let pm = MaxHoldingPeriod::new(5);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 20;
        let bar = make_bar(110.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 20, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::ForceExit);
    }

    #[test]
    fn works_for_shorts() {
        let pm = MaxHoldingPeriod::new(3);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 3;
        let bar = make_bar(90.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 3, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::ForceExit);
    }
}
