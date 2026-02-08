//! Limit-entry execution model — entry at a price offset from the signal bar.
//!
//! Entry orders are Limit: the signal's metadata carries a "reference_price"
//! (or falls back to bar.close). The limit is placed at an offset below (Long)
//! or above (Short) the reference price.
//!
//! This model captures "buy the dip" entries that wait for a pullback.

use crate::components::signal::{SignalDirection, SignalEvent};
use crate::domain::instrument::{round_to_tick, OrderSide};
use crate::domain::{Bar, Instrument, OrderType};

use super::{ExecutionModel, ExecutionPreset, GapPolicy, PathPolicy};

/// Limit-entry model: buy/sell limit at offset from reference price.
#[derive(Debug, Clone)]
pub struct LimitEntryModel {
    preset: ExecutionPreset,
    /// Offset in basis points from the reference price. Default: 25 bps.
    offset_bps: f64,
}

impl LimitEntryModel {
    pub fn new(preset: ExecutionPreset, offset_bps: f64) -> Self {
        Self { preset, offset_bps }
    }

    /// Default: 25 bps offset with realistic friction.
    pub fn default_realistic() -> Self {
        Self::new(ExecutionPreset::Realistic, 25.0)
    }
}

impl Default for LimitEntryModel {
    fn default() -> Self {
        Self::default_realistic()
    }
}

impl ExecutionModel for LimitEntryModel {
    fn name(&self) -> &str {
        "limit_entry"
    }

    fn entry_order_type(
        &self,
        signal: &SignalEvent,
        bar: &Bar,
        instrument: &Instrument,
    ) -> OrderType {
        let reference_price = signal
            .metadata
            .get("reference_price")
            .copied()
            .unwrap_or(bar.close);

        let offset = reference_price * self.offset_bps / 10_000.0;

        let limit_price = match signal.direction {
            // Long: buy below reference (want a pullback). Round down → more favorable to buyer.
            SignalDirection::Long => round_to_tick(
                reference_price - offset,
                instrument.tick_size,
                OrderSide::Sell,
            ),
            // Short: sell above reference (want a bounce). Round up → more favorable to seller.
            SignalDirection::Short => round_to_tick(
                reference_price + offset,
                instrument.tick_size,
                OrderSide::Buy,
            ),
        };

        OrderType::Limit { limit_price }
    }

    fn path_policy(&self) -> PathPolicy {
        self.preset.path_policy()
    }

    fn gap_policy(&self) -> GapPolicy {
        self.preset.gap_policy()
    }

    fn slippage_bps(&self) -> f64 {
        self.preset.slippage_bps()
    }

    fn commission_bps(&self) -> f64 {
        self.preset.commission_bps()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::SignalEventId;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn make_bar() -> Bar {
        Bar {
            symbol: "SPY".into(),
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 98.0,
            close: 103.0,
            volume: 1_000_000,
            adj_close: 103.0,
        }
    }

    fn make_signal(direction: SignalDirection, metadata: HashMap<String, f64>) -> SignalEvent {
        SignalEvent {
            id: SignalEventId(1),
            bar_index: 10,
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "SPY".into(),
            direction,
            strength: 0.8,
            metadata,
        }
    }

    #[test]
    fn long_limit_below_close() {
        let signal = make_signal(SignalDirection::Long, HashMap::new());
        let model = LimitEntryModel::new(ExecutionPreset::Frictionless, 50.0); // 50 bps

        let order_type =
            model.entry_order_type(&signal, &make_bar(), &Instrument::us_equity("SPY"));
        match order_type {
            OrderType::Limit { limit_price } => {
                // close = 103.0, offset = 103 * 50/10000 = 0.515
                // limit = 103 - 0.515 = 102.485, rounded down
                assert!(limit_price < 103.0);
                assert!(limit_price > 102.0);
            }
            _ => panic!("expected Limit"),
        }
    }

    #[test]
    fn short_limit_above_close() {
        let signal = make_signal(SignalDirection::Short, HashMap::new());
        let model = LimitEntryModel::new(ExecutionPreset::Frictionless, 50.0);

        let order_type =
            model.entry_order_type(&signal, &make_bar(), &Instrument::us_equity("SPY"));
        match order_type {
            OrderType::Limit { limit_price } => {
                assert!(limit_price > 103.0);
            }
            _ => panic!("expected Limit"),
        }
    }

    #[test]
    fn uses_reference_price_from_metadata() {
        let mut meta = HashMap::new();
        meta.insert("reference_price".into(), 200.0);
        let signal = make_signal(SignalDirection::Long, meta);
        let model = LimitEntryModel::new(ExecutionPreset::Frictionless, 100.0); // 100 bps = 1%

        let order_type =
            model.entry_order_type(&signal, &make_bar(), &Instrument::us_equity("SPY"));
        match order_type {
            OrderType::Limit { limit_price } => {
                // reference = 200, offset = 200 * 100/10000 = 2.0
                // limit = 200 - 2 = 198.0
                assert!((limit_price - 198.0).abs() < 0.01);
            }
            _ => panic!("expected Limit"),
        }
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(LimitEntryModel::default().name(), "limit_entry");
    }

    #[test]
    fn default_uses_25_bps() {
        let model = LimitEntryModel::default();
        let signal = make_signal(SignalDirection::Long, HashMap::new());
        let order_type =
            model.entry_order_type(&signal, &make_bar(), &Instrument::us_equity("SPY"));
        match order_type {
            OrderType::Limit { limit_price } => {
                // 103 * 25/10000 = 0.2575, limit = 103 - 0.2575 ≈ 102.74
                assert!(limit_price < 103.0);
                assert!(limit_price > 102.5);
            }
            _ => panic!("expected Limit"),
        }
    }
}
