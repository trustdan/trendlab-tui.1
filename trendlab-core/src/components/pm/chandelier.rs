//! Chandelier exit — ATR-based stop measured from the highest high since entry.
//!
//! For longs: raw_stop = highest_high_since_entry - ATR * multiplier.
//! For shorts: raw_stop = lowest_low_since_entry + ATR * multiplier.
//!
//! Key difference from ATR trailing: anchored to the peak/trough since entry,
//! not to the current bar's close. Ratchets faster on strong trends.

use crate::components::indicator::IndicatorValues;
use crate::domain::{Bar, MarketStatus, Position, PositionSide};

use super::{OrderIntent, PositionManager};

/// Chandelier exit position manager.
#[derive(Debug, Clone)]
pub struct Chandelier {
    /// ATR lookback period.
    pub atr_period: usize,
    /// Multiplier applied to ATR (e.g., 3.0).
    pub multiplier: f64,
    /// Precomputed indicator key name.
    indicator_key: String,
}

impl Chandelier {
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

impl PositionManager for Chandelier {
    fn name(&self) -> &str {
        "chandelier_exit"
    }

    fn on_bar(
        &self,
        position: &Position,
        _bar: &Bar,
        bar_index: usize,
        _market_status: MarketStatus,
        indicators: &IndicatorValues,
    ) -> OrderIntent {
        let atr = match indicators.get(&self.indicator_key, bar_index) {
            Some(v) if !v.is_nan() && v > 0.0 => v,
            _ => return OrderIntent::hold(),
        };

        let stop = match position.side {
            PositionSide::Long => position.highest_price_since_entry - atr * self.multiplier,
            PositionSide::Short => position.lowest_price_since_entry + atr * self.multiplier,
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
    fn long_stop_from_highest_high() {
        let pm = Chandelier::new(14, 3.0);
        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 120.0;
        let bar = make_bar(115.0); // current close doesn't matter for stop calc
        let iv = make_indicators(14, 5, 2.0);
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        // stop = 120 - 3*2 = 114
        assert_eq!(intent.stop_price, Some(114.0));
    }

    #[test]
    fn short_stop_from_lowest_low() {
        let pm = Chandelier::new(14, 3.0);
        let mut pos = Position::new_short("SPY".into(), 100.0, 100.0, 0);
        pos.lowest_price_since_entry = 80.0;
        let bar = make_bar(85.0);
        let iv = make_indicators(14, 5, 2.0);
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        // stop = 80 + 3*2 = 86
        assert_eq!(intent.stop_price, Some(86.0));
    }

    #[test]
    fn stop_rises_as_high_watermark_rises() {
        let pm = Chandelier::new(14, 2.0);
        let iv = make_indicators(14, 5, 5.0);

        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 110.0;
        let intent1 = pm.on_bar(&pos, &make_bar(108.0), 5, MarketStatus::Open, &iv);
        // stop = 110 - 2*5 = 100
        assert_eq!(intent1.stop_price, Some(100.0));

        pos.highest_price_since_entry = 120.0;
        let iv2 = make_indicators(14, 6, 5.0);
        let intent2 = pm.on_bar(&pos, &make_bar(118.0), 6, MarketStatus::Open, &iv2);
        // stop = 120 - 2*5 = 110
        assert_eq!(intent2.stop_price, Some(110.0));
    }

    #[test]
    fn holds_when_atr_missing() {
        let pm = Chandelier::new(14, 3.0);
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let bar = make_bar(110.0);
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        assert_eq!(intent.action, super::super::IntentAction::Hold);
    }

    #[test]
    fn chandelier_vs_atr_trailing_difference() {
        // Chandelier uses highest_high, ATR trailing uses close.
        // When close < highest_high, chandelier gives a higher (tighter) stop.
        let chandelier = Chandelier::new(14, 2.0);
        let atr_trail = crate::components::pm::atr_trailing::AtrTrailing::new(14, 2.0);
        let iv = make_indicators(14, 5, 5.0);

        let mut pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        pos.highest_price_since_entry = 120.0;
        let bar = make_bar(110.0); // close = 110, but high was 120

        let ch_intent = chandelier.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);
        let at_intent = atr_trail.on_bar(&pos, &bar, 5, MarketStatus::Open, &iv);

        // chandelier: 120 - 2*5 = 110
        // atr_trail: 110 - 2*5 = 100
        assert_eq!(ch_intent.stop_price, Some(110.0));
        assert_eq!(at_intent.stop_price, Some(100.0));
        // Chandelier is tighter (higher stop for long)
    }
}
