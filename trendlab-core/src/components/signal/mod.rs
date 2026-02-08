//! Signal generation — detects market events, emits directional intent.
//!
//! Signals are portfolio-agnostic: they receive bar history and indicator values,
//! never portfolio or position state. Signal events are immutable once emitted —
//! they describe a market event, not a downstream decision.

pub mod aroon;
pub mod bollinger;
pub mod breakout_52w;
pub mod donchian;
pub mod keltner;
pub mod ma_crossover;
pub mod parabolic_sar;
pub mod roc_momentum;
pub mod supertrend;
pub mod tsmom;

use crate::domain::{Bar, SignalEventId};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::indicator::IndicatorValues;

/// Directional intent of a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalDirection {
    Long,
    Short,
}

/// An immutable market event emitted by a signal generator.
///
/// The metadata payload carries context for downstream components (e.g., breakout level,
/// reference price, signal bar low) without violating portfolio-agnosticism — the signal
/// describes the market event, not the portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub id: SignalEventId,
    pub bar_index: usize,
    pub date: NaiveDate,
    pub symbol: String,
    pub direction: SignalDirection,
    /// Signal strength (0.0 to 1.0, higher = stronger conviction).
    pub strength: f64,
    /// Arbitrary key-value metadata (breakout level, reference price, etc.).
    pub metadata: HashMap<String, f64>,
}

/// Trait for signal generators.
///
/// # Architecture invariant
/// Signals must never reference portfolio state. The `evaluate` method receives
/// only bar history and precomputed indicator values. If a signal implementation
/// needs access to portfolio state, it violates the separation of concerns.
pub trait SignalGenerator: Send + Sync {
    /// Human-readable name (e.g., "donchian_breakout").
    fn name(&self) -> &str;

    /// Number of bars needed before this signal can produce output.
    fn warmup_bars(&self) -> usize;

    /// Evaluate the signal at `bar_index` given the bar history and indicators.
    ///
    /// Returns `Some(SignalEvent)` if a signal fires, `None` otherwise.
    /// The implementation must only use data from `bars[0..=bar_index]`.
    fn evaluate(
        &self,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> Option<SignalEvent>;
}

/// Record of a signal filter evaluating a signal event.
///
/// Kept separate from `SignalEvent` to preserve signal immutability.
/// The same signal event can be evaluated by different filters in different contexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvaluation {
    pub signal_event_id: SignalEventId,
    pub filter_name: String,
    pub verdict: FilterVerdict,
    /// Snapshot of the filter's state at evaluation time (e.g., current ADX value).
    pub filter_state: HashMap<String, f64>,
}

/// Outcome of a signal filter evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterVerdict {
    Passed,
    FilteredByAdx,
    FilteredByRegime,
    FilteredByVolatility,
    FilteredByCustom(String),
}

impl FilterVerdict {
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Null signal — always returns None. Used as a stub in tests that
/// don't need real signal generation.
pub struct NullSignal;

impl SignalGenerator for NullSignal {
    fn name(&self) -> &str {
        "null"
    }

    fn warmup_bars(&self) -> usize {
        0
    }

    fn evaluate(
        &self,
        _bars: &[Bar],
        _bar_index: usize,
        _indicators: &IndicatorValues,
    ) -> Option<SignalEvent> {
        None
    }
}

// Re-export concrete signal types.
pub use aroon::AroonCrossover;
pub use bollinger::BollingerBreakout;
pub use breakout_52w::Breakout52w;
pub use donchian::DonchianBreakout;
pub use keltner::KeltnerBreakout;
pub use ma_crossover::{MaCrossover, MaType};
pub use parabolic_sar::ParabolicSarSignal;
pub use roc_momentum::RocMomentum;
pub use supertrend::SupertrendSignal;
pub use tsmom::Tsmom;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_event_serialization_roundtrip() {
        let mut metadata = HashMap::new();
        metadata.insert("breakout_level".into(), 150.0);
        metadata.insert("signal_bar_low".into(), 145.0);

        let event = SignalEvent {
            id: SignalEventId(1),
            bar_index: 42,
            date: NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "SPY".into(),
            direction: SignalDirection::Long,
            strength: 0.85,
            metadata,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deser: SignalEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.id, deser.id);
        assert_eq!(event.direction, deser.direction);
        assert_eq!(event.strength, deser.strength);
        assert_eq!(event.metadata.len(), deser.metadata.len());
    }

    #[test]
    fn evaluation_references_signal_by_id() {
        let signal_id = SignalEventId(42);
        let eval = SignalEvaluation {
            signal_event_id: signal_id,
            filter_name: "adx_filter".into(),
            verdict: FilterVerdict::FilteredByAdx,
            filter_state: {
                let mut m = HashMap::new();
                m.insert("adx_value".into(), 18.5);
                m
            },
        };
        assert_eq!(eval.signal_event_id, signal_id);
        assert!(!eval.verdict.is_passed());
    }

    #[test]
    fn filter_verdict_is_passed() {
        assert!(FilterVerdict::Passed.is_passed());
        assert!(!FilterVerdict::FilteredByAdx.is_passed());
        assert!(!FilterVerdict::FilteredByCustom("test".into()).is_passed());
    }

    #[test]
    fn null_signal_returns_none() {
        let sig = NullSignal;
        let bars = vec![];
        let iv = IndicatorValues::new();
        assert!(sig.evaluate(&bars, 0, &iv).is_none());
        assert_eq!(sig.name(), "null");
        assert_eq!(sig.warmup_bars(), 0);
    }
}
