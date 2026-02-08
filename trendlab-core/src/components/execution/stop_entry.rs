//! Stop-entry execution model — breakout entries via buy-stop / sell-stop.
//!
//! Entry orders are Stop-Market: the signal's metadata carries a breakout level.
//! For Long signals: buy-stop at breakout level (price must rise to trigger).
//! For Short signals: sell-stop at breakout level (price must fall to trigger).
//!
//! If the signal metadata does not contain a "breakout_level", falls back to
//! bar.high + tick_size (Long) or bar.low - tick_size (Short).

use crate::components::signal::{SignalDirection, SignalEvent};
use crate::domain::instrument::{round_to_tick, OrderSide};
use crate::domain::{Bar, Instrument, OrderType};

use super::{ExecutionModel, ExecutionPreset, GapPolicy, PathPolicy};

/// Stop-entry model: buy/sell stop at signal's breakout level.
#[derive(Debug, Clone)]
pub struct StopEntryModel {
    preset: ExecutionPreset,
}

impl StopEntryModel {
    pub fn new(preset: ExecutionPreset) -> Self {
        Self { preset }
    }
}

impl Default for StopEntryModel {
    fn default() -> Self {
        Self::new(ExecutionPreset::Realistic)
    }
}

impl ExecutionModel for StopEntryModel {
    fn name(&self) -> &str {
        "stop_entry"
    }

    fn entry_order_type(
        &self,
        signal: &SignalEvent,
        bar: &Bar,
        instrument: &Instrument,
    ) -> OrderType {
        let trigger_price = if let Some(&level) = signal.metadata.get("breakout_level") {
            level
        } else {
            // Fallback: one tick above high (long) or below low (short)
            match signal.direction {
                SignalDirection::Long => round_to_tick(
                    bar.high + instrument.tick_size,
                    instrument.tick_size,
                    OrderSide::Buy,
                ),
                SignalDirection::Short => round_to_tick(
                    bar.low - instrument.tick_size,
                    instrument.tick_size,
                    OrderSide::Sell,
                ),
            }
        };

        OrderType::StopMarket { trigger_price }
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
    fn long_with_breakout_level() {
        let mut meta = HashMap::new();
        meta.insert("breakout_level".into(), 106.0);
        let signal = make_signal(SignalDirection::Long, meta);
        let model = StopEntryModel::default();

        let order_type =
            model.entry_order_type(&signal, &make_bar(), &Instrument::us_equity("SPY"));
        match order_type {
            OrderType::StopMarket { trigger_price } => {
                assert_eq!(trigger_price, 106.0);
            }
            _ => panic!("expected StopMarket"),
        }
    }

    #[test]
    fn short_with_breakout_level() {
        let mut meta = HashMap::new();
        meta.insert("breakout_level".into(), 97.0);
        let signal = make_signal(SignalDirection::Short, meta);
        let model = StopEntryModel::default();

        let order_type =
            model.entry_order_type(&signal, &make_bar(), &Instrument::us_equity("SPY"));
        match order_type {
            OrderType::StopMarket { trigger_price } => {
                assert_eq!(trigger_price, 97.0);
            }
            _ => panic!("expected StopMarket"),
        }
    }

    #[test]
    fn long_fallback_above_high() {
        let signal = make_signal(SignalDirection::Long, HashMap::new());
        let model = StopEntryModel::default();
        let instrument = Instrument::us_equity("SPY"); // tick_size = 0.01

        let order_type = model.entry_order_type(&signal, &make_bar(), &instrument);
        match order_type {
            OrderType::StopMarket { trigger_price } => {
                // bar.high = 105.0, + 0.01, rounded up = 105.01
                assert!(trigger_price > 105.0);
            }
            _ => panic!("expected StopMarket"),
        }
    }

    #[test]
    fn short_fallback_below_low() {
        let signal = make_signal(SignalDirection::Short, HashMap::new());
        let model = StopEntryModel::default();
        let instrument = Instrument::us_equity("SPY");

        let order_type = model.entry_order_type(&signal, &make_bar(), &instrument);
        match order_type {
            OrderType::StopMarket { trigger_price } => {
                // bar.low = 98.0, - 0.01, rounded down = 97.99
                assert!(trigger_price < 98.0);
            }
            _ => panic!("expected StopMarket"),
        }
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(StopEntryModel::default().name(), "stop_entry");
    }
}
