//! Close-on-signal execution model — entry at the signal bar's close.
//!
//! Entry orders are Market-On-Close (MOC): signal at bar T → MOC order fills
//! at T's close (end-of-bar phase). This gives the fastest possible execution
//! but requires the signal to fire during intrabar evaluation or at start-of-bar.

use crate::components::signal::SignalEvent;
use crate::domain::{Bar, Instrument, OrderType};

use super::{ExecutionModel, ExecutionPreset, GapPolicy, PathPolicy};

/// Market-On-Close entry: signal evaluated → MOC fill at bar close.
#[derive(Debug, Clone)]
pub struct CloseOnSignalModel {
    preset: ExecutionPreset,
}

impl CloseOnSignalModel {
    pub fn new(preset: ExecutionPreset) -> Self {
        Self { preset }
    }
}

impl Default for CloseOnSignalModel {
    fn default() -> Self {
        Self::new(ExecutionPreset::Realistic)
    }
}

impl ExecutionModel for CloseOnSignalModel {
    fn name(&self) -> &str {
        "close_on_signal"
    }

    fn entry_order_type(
        &self,
        _signal: &SignalEvent,
        _bar: &Bar,
        _instrument: &Instrument,
    ) -> OrderType {
        OrderType::MarketOnClose
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
    use crate::components::signal::SignalDirection;
    use crate::domain::ids::SignalEventId;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn make_signal() -> SignalEvent {
        SignalEvent {
            id: SignalEventId(1),
            bar_index: 10,
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "SPY".into(),
            direction: SignalDirection::Long,
            strength: 0.8,
            metadata: HashMap::new(),
        }
    }

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

    #[test]
    fn produces_moc_order() {
        let model = CloseOnSignalModel::default();
        let order_type =
            model.entry_order_type(&make_signal(), &make_bar(), &Instrument::us_equity("SPY"));
        assert!(matches!(order_type, OrderType::MarketOnClose));
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(CloseOnSignalModel::default().name(), "close_on_signal");
    }

    #[test]
    fn frictionless_preset() {
        let model = CloseOnSignalModel::new(ExecutionPreset::Frictionless);
        assert_eq!(model.slippage_bps(), 0.0);
        assert_eq!(model.commission_bps(), 0.0);
    }
}
