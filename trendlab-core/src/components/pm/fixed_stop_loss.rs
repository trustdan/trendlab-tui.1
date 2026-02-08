//! Fixed stop-loss — simple stop at a fixed percentage below entry.
//!
//! The stop is placed once when first evaluated and never adjusted.
//! For longs: stop = entry * (1 - stop_pct).
//! For shorts: stop = entry * (1 + stop_pct).

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Fixed stop-loss position manager.
#[derive(Debug, Clone)]
pub struct FixedStopLoss {
    /// Stop distance as a fraction (e.g., 0.05 for 5%).
    pub stop_pct: f64,
}

impl FixedStopLoss {
    pub fn new(stop_pct: f64) -> Self {
        assert!(stop_pct > 0.0, "stop_pct must be positive");
        assert!(stop_pct < 1.0, "stop_pct must be < 1.0");
        Self { stop_pct }
    }
}

impl PositionManager for FixedStopLoss {
    fn name(&self) -> &str {
        "fixed_stop_loss"
    }

    fn on_bar(
        &self,
        position: &Position,
        _bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        // Only set the stop once — if already set, hold.
        if position.current_stop.is_some() {
            return OrderIntent::hold();
        }

        let stop = match position.side {
            PositionSide::Long => position.avg_entry_price * (1.0 - self.stop_pct),
            PositionSide::Short => position.avg_entry_price * (1.0 + self.stop_pct),
            PositionSide::Flat => return OrderIntent::hold(),
        };

        OrderIntent::adjust_stop(stop)
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
    fn long_stop_below_entry() {
        let pm = FixedStopLoss::new(0.05);
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(102.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::AdjustStop);
        assert_eq!(intent.stop_price, Some(95.0));
    }

    #[test]
    fn short_stop_above_entry() {
        let pm = FixedStopLoss::new(0.05);
        let pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(98.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::AdjustStop);
        assert_eq!(intent.stop_price, Some(105.0));
    }

    #[test]
    fn holds_after_initial_placement() {
        let pm = FixedStopLoss::new(0.05);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.current_stop = Some(95.0); // already set
        let bar = make_bar(110.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn flat_position_holds() {
        let pm = FixedStopLoss::new(0.05);
        let pos = Position {
            symbol: "SPY".into(),
            side: PositionSide::Flat,
            quantity: 0.0,
            avg_entry_price: 100.0,
            entry_bar: 0,
            highest_price_since_entry: 100.0,
            lowest_price_since_entry: 100.0,
            bars_held: 0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            current_stop: None,
        };
        let bar = make_bar(100.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 0, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }
}
