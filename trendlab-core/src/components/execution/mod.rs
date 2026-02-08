//! Execution model — determines how orders get filled.
//!
//! The execution model specifies what order type to use for entries,
//! the path policy for intrabar ambiguity, gap handling, and friction
//! parameters (slippage, commission).

pub mod close_on_signal;
pub mod limit_entry;
pub mod next_bar_open;
pub mod stop_entry;

use crate::domain::{Bar, Instrument, OrderType};

use super::signal::SignalEvent;
use serde::{Deserialize, Serialize};

/// Intrabar path policy for resolving ambiguous bars.
///
/// When a bar could trigger both a stop-loss and take-profit, the path
/// policy determines which outcome is assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathPolicy {
    /// OHLC ordering, no ambiguity resolution needed.
    Deterministic,
    /// Adversarial: assume the worse outcome happened first (default).
    WorstCase,
    /// Optimistic: assume the better outcome happened first.
    BestCase,
}

/// Policy for handling gap-through fills.
///
/// When price gaps through a stop level at the open, how to determine fill price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapPolicy {
    /// Fill at the open price (worse than trigger — realistic default).
    FillAtOpen,
    /// Fill at the trigger level (optimistic).
    FillAtTrigger,
    /// Fill at the worst of open and trigger.
    FillAtWorst,
}

/// Trait for execution models.
///
/// The execution model determines:
/// 1. What order type to use for entries (market, stop, limit, close-on-signal)
/// 2. Path policy for intrabar ambiguity
/// 3. Gap policy for gap-through fills
/// 4. Friction parameters (slippage bps, commission bps)
pub trait ExecutionModel: Send + Sync {
    /// Human-readable name (e.g., "next_bar_open", "stop_entry").
    fn name(&self) -> &str;

    /// Create the entry order type for a given signal.
    fn entry_order_type(
        &self,
        signal: &SignalEvent,
        bar: &Bar,
        instrument: &Instrument,
    ) -> OrderType;

    /// Intrabar path policy.
    fn path_policy(&self) -> PathPolicy;

    /// Gap-through fill policy.
    fn gap_policy(&self) -> GapPolicy;

    /// Slippage in basis points (applied directionally).
    fn slippage_bps(&self) -> f64;

    /// Commission in basis points per side.
    fn commission_bps(&self) -> f64;
}

/// Named execution presets bundling path policy, slippage, and commission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionPreset {
    /// Zero friction: no slippage, no commission, deterministic path.
    Frictionless,
    /// Realistic: moderate slippage, standard commission, worst-case path.
    Realistic,
    /// Hostile: high slippage, high commission, worst-case path.
    Hostile,
    /// Optimistic: low slippage, low commission, best-case path.
    Optimistic,
}

impl ExecutionPreset {
    pub fn path_policy(self) -> PathPolicy {
        match self {
            Self::Frictionless => PathPolicy::Deterministic,
            Self::Realistic | Self::Hostile => PathPolicy::WorstCase,
            Self::Optimistic => PathPolicy::BestCase,
        }
    }

    pub fn gap_policy(self) -> GapPolicy {
        match self {
            Self::Frictionless | Self::Optimistic => GapPolicy::FillAtTrigger,
            Self::Realistic | Self::Hostile => GapPolicy::FillAtOpen,
        }
    }

    pub fn slippage_bps(self) -> f64 {
        match self {
            Self::Frictionless => 0.0,
            Self::Realistic => 5.0,
            Self::Hostile => 20.0,
            Self::Optimistic => 2.0,
        }
    }

    pub fn commission_bps(self) -> f64 {
        match self {
            Self::Frictionless => 0.0,
            Self::Realistic => 5.0,
            Self::Hostile => 15.0,
            Self::Optimistic => 2.0,
        }
    }
}

// Re-export concrete models.
pub use close_on_signal::CloseOnSignalModel;
pub use limit_entry::LimitEntryModel;
pub use next_bar_open::NextBarOpenModel;
pub use stop_entry::StopEntryModel;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_produce_different_friction() {
        assert_eq!(ExecutionPreset::Frictionless.slippage_bps(), 0.0);
        assert!(ExecutionPreset::Realistic.slippage_bps() > 0.0);
        assert!(
            ExecutionPreset::Hostile.slippage_bps() > ExecutionPreset::Realistic.slippage_bps()
        );
    }

    #[test]
    fn worst_case_is_default_for_realistic() {
        assert_eq!(
            ExecutionPreset::Realistic.path_policy(),
            PathPolicy::WorstCase
        );
    }

    #[test]
    fn preset_serialization_roundtrip() {
        let preset = ExecutionPreset::Realistic;
        let json = serde_json::to_string(&preset).unwrap();
        let deser: ExecutionPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(preset, deser);
    }
}
