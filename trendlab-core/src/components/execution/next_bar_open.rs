//! Next-bar-open execution model — the default.
//!
//! Entry orders are Market-On-Open (MOO): signal at bar T's close → fill at
//! bar T+1's open. This is the most conservative and realistic default.

use crate::components::signal::SignalEvent;
use crate::domain::{Bar, Instrument, OrderType};

use super::{ExecutionModel, ExecutionPreset, GapPolicy, PathPolicy};

/// Market-On-Open entry: signal evaluated at bar T → MOO order fills at T+1.
#[derive(Debug, Clone)]
pub struct NextBarOpenModel {
    preset: ExecutionPreset,
}

impl NextBarOpenModel {
    pub fn new(preset: ExecutionPreset) -> Self {
        Self { preset }
    }

    /// Default: realistic friction.
    pub fn default_realistic() -> Self {
        Self::new(ExecutionPreset::Realistic)
    }
}

impl Default for NextBarOpenModel {
    fn default() -> Self {
        Self::new(ExecutionPreset::Realistic)
    }
}

impl ExecutionModel for NextBarOpenModel {
    fn name(&self) -> &str {
        "next_bar_open"
    }

    fn entry_order_type(
        &self,
        _signal: &SignalEvent,
        _bar: &Bar,
        _instrument: &Instrument,
    ) -> OrderType {
        OrderType::MarketOnOpen
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
    fn produces_moo_order() {
        let model = NextBarOpenModel::default();
        let order_type =
            model.entry_order_type(&make_signal(), &make_bar(), &Instrument::us_equity("SPY"));
        assert!(matches!(order_type, OrderType::MarketOnOpen));
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(NextBarOpenModel::default().name(), "next_bar_open");
    }

    #[test]
    fn realistic_preset_friction() {
        let model = NextBarOpenModel::default_realistic();
        assert!(model.slippage_bps() > 0.0);
        assert!(model.commission_bps() > 0.0);
        assert_eq!(model.path_policy(), PathPolicy::WorstCase);
    }

    #[test]
    fn frictionless_preset() {
        let model = NextBarOpenModel::new(ExecutionPreset::Frictionless);
        assert_eq!(model.slippage_bps(), 0.0);
        assert_eq!(model.commission_bps(), 0.0);
        assert_eq!(model.path_policy(), PathPolicy::Deterministic);
    }
}
