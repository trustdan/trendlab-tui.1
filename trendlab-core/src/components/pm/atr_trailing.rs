//! ATR trailing stop — trail at close minus ATR times a multiplier.
//!
//! For longs: raw_stop = close - ATR * multiplier.
//! For shorts: raw_stop = close + ATR * multiplier.
//!
//! Requires a precomputed ATR indicator (e.g., "atr_14") in the indicator set.
//! If the ATR value is unavailable or NaN, returns Hold.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// ATR trailing stop position manager.
#[derive(Debug, Clone)]
pub struct AtrTrailing {
    /// ATR lookback period (must match an `Atr` indicator in the indicator set).
    pub atr_period: usize,
    /// Multiplier applied to ATR (e.g., 3.0 for 3x ATR).
    pub multiplier: f64,
    /// Precomputed indicator key name.
    indicator_key: String,
}

impl AtrTrailing {
    pub fn new(atr_period: usize, multiplier: f64) -> Self {
        assert!(atr_period >= 1, "atr_period must be >= 1");
        assert!(multiplier > 0.0, "multiplier must be positive");
        Self {
            atr_period,
            multiplier,
            indicator_key: format!("atr_{atr_period}"),
        }
    }
}

impl PositionManager for AtrTrailing {
    fn name(&self) -> &str {
        "atr_trailing"
    }

    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        bar_index: usize,
        _market_status: MarketStatus,
        indicators: &IndicatorValues,
    ) -> OrderIntent {
        let atr = match indicators.get(&self.indicator_key, bar_index) {
            Some(v) if !v.is_nan() && v > 0.0 => v,
            _ => return OrderIntent::hold(), // ATR not available
        };

        let stop = match position.side {
            PositionSide::Long => bar.close - atr * self.multiplier,
            PositionSide::Short => bar.close + atr * self.multiplier,
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

    fn make_indicators(atr_period: usize, bar_index: usize, atr_value: f64) -> IndicatorValues {
        let mut iv = IndicatorValues::new();
        let mut series = vec![f64::NAN; bar_index + 1];
        series[bar_index] = atr_value;
        iv.insert(format!("atr_{atr_period}"), series);
        iv
    }

    #[test]
    fn long_stop_below_close() {
        let pm = AtrTrailing::new(14, 3.0);
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(110.0);
        let iv = make_indicators(14, 5, 2.0); // ATR = 2.0
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        // stop = 110 - 3*2 = 104
        assert_eq!(intent.stop_price, Some(104.0));
    }

    #[test]
    fn short_stop_above_close() {
        let pm = AtrTrailing::new(14, 3.0);
        let pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(90.0);
        let iv = make_indicators(14, 5, 2.0);
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        // stop = 90 + 3*2 = 96
        assert_eq!(intent.stop_price, Some(96.0));
    }

    #[test]
    fn holds_when_atr_missing() {
        let pm = AtrTrailing::new(14, 3.0);
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(110.0);
        let iv = IndicatorValues::new(); // no ATR
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn holds_when_atr_nan() {
        let pm = AtrTrailing::new(14, 3.0);
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(110.0);
        let iv = make_indicators(14, 5, f64::NAN);
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn stop_rises_as_price_rises() {
        let pm = AtrTrailing::new(14, 2.0);
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let iv = make_indicators(14, 5, 5.0);
        let bar1 = make_bar(100.0);
        let intent1 = pm.on_bar(&pos, &bar1, 5, MarketStatus::Open, &iv);
        // stop = 100 - 2*5 = 90
        assert_eq!(intent1.stop_price, Some(90.0));

        let iv2 = make_indicators(14, 6, 5.0);
        let bar2 = make_bar(110.0);
        let intent2 = pm.on_bar(&pos, &bar2, 6, MarketStatus::Open, &iv2);
        // stop = 110 - 2*5 = 100
        assert_eq!(intent2.stop_price, Some(100.0));
    }
}
