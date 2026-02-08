//! Time decay stop — the stop tightens over time.
//!
//! The effective stop distance starts at `initial_pct` and decays by
//! `decay_per_bar` each bar, with a floor at `min_pct`.
//!
//! For longs: raw_stop = close * (1 - effective_pct).
//! The ratchet in the engine ensures the absolute stop level never drops.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Time decay stop position manager.
#[derive(Debug, Clone)]
pub struct TimeDecay {
    /// Starting stop distance as a fraction (e.g., 0.10 for 10%).
    pub initial_pct: f64,
    /// Decay per bar (e.g., 0.001 = stop tightens 0.1% per bar).
    pub decay_per_bar: f64,
    /// Minimum stop distance (floor). The stop never gets closer than this.
    pub min_pct: f64,
}

impl TimeDecay {
    pub fn new(initial_pct: f64, decay_per_bar: f64, min_pct: f64) -> Self {
        assert!(initial_pct > 0.0, "initial_pct must be positive");
        assert!(initial_pct < 1.0, "initial_pct must be < 1.0");
        assert!(decay_per_bar > 0.0, "decay_per_bar must be positive");
        assert!(min_pct >= 0.0, "min_pct must be non-negative");
        assert!(min_pct < initial_pct, "min_pct must be < initial_pct");
        Self {
            initial_pct,
            decay_per_bar,
            min_pct,
        }
    }

    /// Compute the effective percentage at the current bars_held.
    fn effective_pct(&self, bars_held: usize) -> f64 {
        let raw = self.initial_pct - (bars_held as f64 * self.decay_per_bar);
        raw.max(self.min_pct)
    }
}

impl PositionManager for TimeDecay {
    fn name(&self) -> &str {
        "time_decay"
    }

    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        let pct = self.effective_pct(position.bars_held);

        let stop = match position.side {
            PositionSide::Long => bar.close * (1.0 - pct),
            PositionSide::Short => bar.close * (1.0 + pct),
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
    fn effective_pct_decays() {
        let pm = TimeDecay::new(0.10, 0.001, 0.02);
        assert!((pm.effective_pct(0) - 0.10).abs() < 1e-10);
        assert!((pm.effective_pct(10) - 0.09).abs() < 1e-10);
        assert!((pm.effective_pct(50) - 0.05).abs() < 1e-10);
    }

    #[test]
    fn effective_pct_floors_at_min() {
        let pm = TimeDecay::new(0.10, 0.001, 0.02);
        // At bar 80: 0.10 - 80*0.001 = 0.02 (exactly min)
        assert!((pm.effective_pct(80) - 0.02).abs() < 1e-10);
        // At bar 100: would be 0.0, clamped to 0.02
        assert!((pm.effective_pct(100) - 0.02).abs() < 1e-10);
        // At bar 200: still 0.02
        assert!((pm.effective_pct(200) - 0.02).abs() < 1e-10);
    }

    #[test]
    fn long_stop_tightens_over_time() {
        let pm = TimeDecay::new(0.10, 0.01, 0.02);
        let bar = make_bar(100.0);
        let iv = IndicatorValues::new();

        // Bar 0: stop = 100 * (1 - 0.10) = 90
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 0;
        let intent = pm.on_bar(&pos, &bar, 0, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(90.0));

        // Bar 4: stop = 100 * (1 - 0.06) = 94 (tighter)
        pos.bars_held = 4;
        let intent = pm.on_bar(&pos, &bar, 4, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(94.0));

        // Bar 8: stop = 100 * (1 - 0.02) = 98 (at floor)
        pos.bars_held = 8;
        let intent = pm.on_bar(&pos, &bar, 8, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(98.0));
    }

    #[test]
    fn short_stop_tightens_over_time() {
        let pm = TimeDecay::new(0.10, 0.01, 0.02);
        let bar = make_bar(100.0);
        let iv = IndicatorValues::new();

        // Bar 0: stop = 100 * (1 + 0.10) = 110
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 0;
        let intent = pm.on_bar(&pos, &bar, 0, MarketStatus::Open, &iv);
        let stop = intent.stop_price.unwrap();
        assert!((stop - 110.0).abs() < 1e-10, "expected ~110, got {stop}");

        // Bar 4: stop = 100 * (1 + 0.06) = 106 (tighter for short)
        pos.bars_held = 4;
        let intent = pm.on_bar(&pos, &bar, 4, MarketStatus::Open, &iv);
        let stop = intent.stop_price.unwrap();
        assert!((stop - 106.0).abs() < 1e-10, "expected ~106, got {stop}");
    }

    #[test]
    fn convergence_to_min() {
        let pm = TimeDecay::new(0.10, 0.001, 0.02);
        let iv = IndicatorValues::new();
        // At bar 80, effective_pct = 0.02, so stop = 100 * 0.98 = 98
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.bars_held = 80;
        let intent = pm.on_bar(&pos, &make_bar(100.0), 80, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(98.0));

        // At bar 200, still 98 (floor)
        pos.bars_held = 200;
        let intent = pm.on_bar(&pos, &make_bar(100.0), 200, MarketStatus::Open, &iv);
        assert_eq!(intent.stop_price, Some(98.0));
    }
}
