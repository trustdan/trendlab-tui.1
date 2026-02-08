//! Position management — manages open positions, emits exit/adjustment intents.
//!
//! PMs operate after the post-bar mark-to-market step. They emit order intents
//! (not direct fills) that apply to the NEXT bar. PMs must obey the ratchet
//! invariant: stops may tighten but never loosen.
//!
//! ## Concrete implementations
//!
//! - [`AtrTrailing`] — ATR-based trailing stop
//! - [`Chandelier`] — chandelier exit (ATR from highest high since entry)
//! - [`PercentTrailing`] — fixed percentage trailing stop
//! - [`SinceEntryTrailing`] — drawdown-from-peak condition exit
//! - [`FrozenReference`] — stop frozen at entry price, never moves
//! - [`TimeDecay`] — stop tightens over time
//! - [`MaxHoldingPeriod`] — force exit after N bars
//! - [`FixedStopLoss`] — simple fixed stop below entry
//! - [`BreakevenThenTrail`] — move to breakeven, then trail

pub mod atr_trailing;
pub mod breakeven_then_trail;
pub mod chandelier;
pub mod fixed_stop_loss;
pub mod frozen_reference;
pub mod max_holding_period;
pub mod percent_trailing;
pub mod since_entry_trailing;
pub mod time_decay;

pub use atr_trailing::AtrTrailing;
pub use breakeven_then_trail::BreakevenThenTrail;
pub use chandelier::Chandelier;
pub use fixed_stop_loss::FixedStopLoss;
pub use frozen_reference::FrozenReference;
pub use max_holding_period::MaxHoldingPeriod;
pub use percent_trailing::PercentTrailing;
pub use since_entry_trailing::SinceEntryTrailing;
pub use time_decay::TimeDecay;

use crate::domain::{Bar, MarketStatus, Position};

use super::indicator::IndicatorValues;
use serde::{Deserialize, Serialize};

/// What action the position manager wants to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentAction {
    /// Keep the current stop/target unchanged.
    Hold,
    /// Adjust the stop price (tighten only — ratchet invariant).
    AdjustStop,
    /// Adjust the take-profit target.
    AdjustTarget,
    /// Force exit on the next bar (max holding period, time decay converged, etc.).
    ForceExit,
}

/// Order intent emitted by a position manager.
///
/// Translated into cancel/replace orders on the order book. The cancel/replace
/// is atomic: no "stopless window" between cancellation and new order placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderIntent {
    pub action: IntentAction,
    /// New stop price (only meaningful when action is AdjustStop).
    pub stop_price: Option<f64>,
    /// New target price (only meaningful when action is AdjustTarget).
    pub target_price: Option<f64>,
}

impl OrderIntent {
    pub fn hold() -> Self {
        Self {
            action: IntentAction::Hold,
            stop_price: None,
            target_price: None,
        }
    }

    pub fn adjust_stop(price: f64) -> Self {
        Self {
            action: IntentAction::AdjustStop,
            stop_price: Some(price),
            target_price: None,
        }
    }

    pub fn force_exit() -> Self {
        Self {
            action: IntentAction::ForceExit,
            stop_price: None,
            target_price: None,
        }
    }
}

/// Trait for position managers.
///
/// # Architecture invariants
/// - PMs operate after post-bar mark-to-market, emitting intents for the NEXT bar.
/// - PMs must obey the ratchet invariant: stops may tighten but never loosen.
/// - On void bars (`MarketStatus::Closed`), the engine does NOT call `on_bar`.
///   Time-based counters (bars_held) are already incremented by `Position::tick_bar()`.
/// - Time-based exits that expire during void bars emit on the next valid bar.
pub trait PositionManager: Send + Sync {
    /// Human-readable name (e.g., "atr_trailing", "chandelier_exit").
    fn name(&self) -> &str;

    /// Evaluate the position and return an order intent for the next bar.
    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        bar_index: usize,
        market_status: MarketStatus,
        indicators: &IndicatorValues,
    ) -> OrderIntent;
}

/// No-op position manager — always holds. Used as default in tests
/// and for strategies that rely solely on signal-driven exits.
pub struct NoOpPm;

impl PositionManager for NoOpPm {
    fn name(&self) -> &str {
        "no_op"
    }

    fn on_bar(
        &self,
        _position: &Position,
        _bar: &Bar,
        _bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        OrderIntent::hold()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_intent() {
        let intent = OrderIntent::hold();
        assert_eq!(intent.action, IntentAction::Hold);
        assert!(intent.stop_price.is_none());
    }

    #[test]
    fn adjust_stop_intent() {
        let intent = OrderIntent::adjust_stop(95.0);
        assert_eq!(intent.action, IntentAction::AdjustStop);
        assert_eq!(intent.stop_price, Some(95.0));
    }

    #[test]
    fn force_exit_intent() {
        let intent = OrderIntent::force_exit();
        assert_eq!(intent.action, IntentAction::ForceExit);
    }

    #[test]
    fn intent_serialization_roundtrip() {
        let intent = OrderIntent::adjust_stop(102.5);
        let json = serde_json::to_string(&intent).unwrap();
        let deser: OrderIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(intent.action, deser.action);
        assert_eq!(intent.stop_price, deser.stop_price);
    }

    #[test]
    fn noop_pm_always_holds() {
        use chrono::NaiveDate;
        let pm = NoOpPm;
        let pos = Position::new_long("SPY".into(), 100.0, 100.0, 0);
        let bar = Bar {
            symbol: "SPY".to_string(),
            date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 100.0,
            high: 105.0,
            low: 98.0,
            close: 103.0,
            volume: 1000,
            adj_close: 103.0,
        };
        let iv = IndicatorValues::new();
        let intent = pm.on_bar(&pos, &bar, 0, MarketStatus::Open, &iv);
        assert_eq!(intent.action, IntentAction::Hold);
    }
}
