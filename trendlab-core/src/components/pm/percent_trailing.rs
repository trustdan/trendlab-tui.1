//! Percent trailing stop — trail at a fixed percentage below the high watermark.
//!
//! For longs: stop = highest_price_since_entry * (1 - trail_pct).
//! For shorts: stop = lowest_price_since_entry * (1 + trail_pct).
//!
//! The ratchet invariant is enforced by the engine (loop_runner), not here.
//! This PM always emits the raw desired stop; the engine clamps it.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Percent trailing stop position manager.
#[derive(Debug, Clone)]
pub struct PercentTrailing {
    /// Trail distance as a fraction (e.g., 0.10 for 10%).
    pub trail_pct: f64,
}

impl PercentTrailing {
    pub fn new(trail_pct: f64) -> Self {
        assert!(trail_pct > 0.0, "trail_pct must be positive");
        assert!(trail_pct < 1.0, "trail_pct must be < 1.0");
        Self { trail_pct }
    }
}

impl PositionManager for PercentTrailing {
    fn name(&self) -> &str {
        "percent_trailing"
    }

    fn on_bar(
        &self,
        position: &Position,
        _bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        let stop = match position.side {
            PositionSide::Long => position.highest_price_since_entry * (1.0 - self.trail_pct),
            PositionSide::Short => position.lowest_price_since_entry * (1.0 + self.trail_pct),
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
    fn long_stop_trails_high() {
        let pm = PercentTrailing::new(0.10);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 120.0;
        let bar = make_bar(115.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(108.0)); // 120 * 0.9
    }

    #[test]
    fn long_stop_rises_with_new_high() {
        let pm = PercentTrailing::new(0.10);
        let iv = IndicatorValues::new();

        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 100.0;
        let intent = pm.on_bar(&pos, &make_bar(100.0), 1, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(90.0)); // 100 * 0.9

        pos.highest_price_since_entry = 110.0;
        let intent = pm.on_bar(&pos, &make_bar(110.0), 2, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(99.0)); // 110 * 0.9
    }

    #[test]
    fn short_stop_trails_low() {
        let pm = PercentTrailing::new(0.10);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.lowest_price_since_entry = 80.0;
        let bar = make_bar(85.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(88.0)); // 80 * 1.1
    }
}
