//! Position management — manages open positions, emits exit/adjustment intents.
//!
//! PMs operate after the post-bar mark-to-market step. They emit order intents
//! (not direct fills) that apply to the NEXT bar. PMs must obey the ratchet
//! invariant: stops may tighten but never loosen.

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
/// - On void bars (`MarketStatus::Closed`), PMs may increment time-based counters
///   but must NOT emit price-dependent order intents.
/// - Time-based exits that expire during void bars emit on the next valid bar.
pub trait PositionManager: Send + Sync {
    /// Human-readable name (e.g., "atr_trailing", "chandelier_exit").
    fn name(&self) -> &str;

    /// Evaluate the position and return an order intent for the next bar.
    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        market_status: MarketStatus,
        indicators: &IndicatorValues,
    ) -> OrderIntent;
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
}
