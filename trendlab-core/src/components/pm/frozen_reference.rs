//! Frozen reference exit — stop at a fixed percentage below entry, never moves.
//!
//! Identical mechanism to fixed_stop_loss but with different semantic framing.
//! The stop is placed once on first evaluation and never adjusted.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Frozen reference exit position manager.
#[derive(Debug, Clone)]
pub struct FrozenReference {
    /// Exit distance as a fraction (e.g., 0.08 for 8%).
    pub exit_pct: f64,
}

impl FrozenReference {
    pub fn new(exit_pct: f64) -> Self {
        assert!(exit_pct > 0.0, "exit_pct must be positive");
        assert!(exit_pct < 1.0, "exit_pct must be < 1.0");
        Self { exit_pct }
    }
}

impl PositionManager for FrozenReference {
    fn name(&self) -> &str {
        "frozen_reference"
    }

    fn on_bar(
        &self,
        position: &Position,
        _bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        // Only set the stop once — if already set, hold forever.
        if position.current_stop.is_some() {
            return OrderIntent::hold();
        }

        let stop = match position.side {
            PositionSide::Long => position.avg_entry_price * (1.0 - self.exit_pct),
            PositionSide::Short => position.avg_entry_price * (1.0 + self.exit_pct),
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
    fn long_stop_frozen_at_entry() {
        let pm = FrozenReference::new(0.08);
        let pos = Position::new_long("SPY".into(), 100.0, 200.0, 0);
        let bar = make_bar(210.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(200.0 * 0.92));
    }

    #[test]
    fn never_moves_after_first_bar() {
        let pm = FrozenReference::new(0.10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let iv = IndicatorValues::new();

        // First bar: sets stop
        let bar = make_bar(105.0);
        let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(90.0));

        // Simulate engine setting current_stop
        pos.current_stop = Some(90.0);

        // Subsequent bars: always holds
        for t in 2..200 {
            let bar = make_bar(100.0 + t as f64);
            let intent = pm.on_bar(&pos, &bar, t, MarketStatus::Open, &iv);
            assert_eq!(intent.action, super::super::IntentAction::Hold);
        }
    }

    #[test]
    fn short_stop_frozen_above_entry() {
        let pm = FrozenReference::new(0.08);
        let pos = Position::new_short("SPY".into(), 100.0, 200.0, 0);
        let bar = make_bar(190.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 1, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(200.0 * 1.08));
    }
}
