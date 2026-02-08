//! Since-entry trailing — condition-based exit when drawdown from peak exceeds threshold.
//!
//! Unlike percent_trailing (which places a stop order), this PM monitors the
//! drawdown from the highest price since entry and emits ForceExit when the
//! threshold is breached. The exit translates to a MOO order on the next bar.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Since-entry trailing position manager.
#[derive(Debug, Clone)]
pub struct SinceEntryTrailing {
    /// Exit threshold as a fraction (e.g., 0.15 for 15% drawdown from peak).
    pub exit_pct: f64,
}

impl SinceEntryTrailing {
    pub fn new(exit_pct: f64) -> Self {
        assert!(exit_pct > 0.0, "exit_pct must be positive");
        assert!(exit_pct < 1.0, "exit_pct must be < 1.0");
        Self { exit_pct }
    }
}

impl PositionManager for SinceEntryTrailing {
    fn name(&self) -> &str {
        "since_entry_trailing"
    }

    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        match position.side {
            PositionSide::Long => {
                let peak = position.highest_price_since_entry;
                if peak <= 0.0 {
                    return OrderIntent::hold();
                }
                let drawdown = (peak - bar.close) / peak;
                if drawdown >= self.exit_pct {
                    OrderIntent::force_exit()
                } else {
                    OrderIntent::hold()
                }
            }
            PositionSide::Short => {
                let trough = position.lowest_price_since_entry;
                if trough <= 0.0 {
                    return OrderIntent::hold();
                }
                // For shorts, "drawdown" is price rising from the trough
                let drawup = (bar.close - trough) / trough;
                if drawup >= self.exit_pct {
                    OrderIntent::force_exit()
                } else {
                    OrderIntent::hold()
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
    fn long_holds_within_threshold() {
        let pm = SinceEntryTrailing::new(0.15);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 120.0;
        // Close at 105: drawdown = (120-105)/120 = 12.5% < 15%
        let bar = make_bar(105.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn long_exits_at_threshold() {
        let pm = SinceEntryTrailing::new(0.15);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 120.0;
        // Close at 102: drawdown = (120-102)/120 = 15% == threshold
        let bar = make_bar(102.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::ForceExit);
    }

    #[test]
    fn long_exits_beyond_threshold() {
        let pm = SinceEntryTrailing::new(0.10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 100.0;
        // Close at 85: drawdown = 15% > 10%
        let bar = make_bar(85.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::ForceExit);
    }

    #[test]
    fn short_holds_within_threshold() {
        let pm = SinceEntryTrailing::new(0.15);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.lowest_price_since_entry = 80.0;
        // Close at 88: drawup = (88-80)/80 = 10% < 15%
        let bar = make_bar(88.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn short_exits_beyond_threshold() {
        let pm = SinceEntryTrailing::new(0.15);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.lowest_price_since_entry = 80.0;
        // Close at 95: drawup = (95-80)/80 = 18.75% > 15%
        let bar = make_bar(95.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::ForceExit);
    }
}
