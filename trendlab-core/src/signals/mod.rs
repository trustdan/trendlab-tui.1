//! Signal generation — portfolio-agnostic indicators
//!
//! Signals must NEVER depend on portfolio state (positions, equity, etc.).
//! They represent pure market timing logic based on OHLC data only.

pub mod intent;
pub mod examples;

pub use intent::{SignalIntent, SignalStrength};

use crate::domain::Bar;

/// Portfolio-agnostic signal generator
///
/// # Invariants
/// - `generate()` MUST NOT access portfolio state
/// - `generate()` MUST be deterministic for the same bar sequence
/// - Signals represent "what do I want?" not "what do I have?"
pub trait Signal: Send + Sync {
    /// Generate signal intent based ONLY on market data
    ///
    /// # Arguments
    /// - `bars`: Historical bar data (includes current bar)
    ///
    /// # Returns
    /// Signal intent (Long/Short/Flat) with optional strength
    fn generate(&self, bars: &[Bar]) -> SignalIntent;

    /// Signal name for manifest/logging
    fn name(&self) -> &str;

    /// Maximum lookback period required (in bars)
    ///
    /// Used for warmup validation and cache invalidation.
    /// Example: MA(200) requires 200 bars minimum.
    fn max_lookback(&self) -> usize;

    /// Signal family classification for order policy matching
    ///
    /// Examples:
    /// - Breakout signals → stop entries
    /// - Mean-reversion signals → limit entries
    /// - Trend-following → market/stop mix
    fn signal_family(&self) -> SignalFamily;
}

/// Signal classification for natural order type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalFamily {
    /// Breakout/momentum (enter on continuation)
    /// Natural entry: StopMarket above/below breakout level
    Breakout,

    /// Mean-reversion (enter on pullback)
    /// Natural entry: Limit orders at favorable prices
    MeanReversion,

    /// Trend-following (directional bias)
    /// Natural entry: Market or trailing stop
    Trend,

    /// Unclassified (use immediate policy)
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Bar;
    use chrono::Utc;

    struct DummySignal;

    impl Signal for DummySignal {
        fn generate(&self, _bars: &[Bar]) -> SignalIntent {
            SignalIntent::Flat
        }

        fn name(&self) -> &str {
            "dummy"
        }

        fn max_lookback(&self) -> usize {
            0
        }

        fn signal_family(&self) -> SignalFamily {
            SignalFamily::Other
        }
    }

    #[test]
    fn test_signal_trait_compiles() {
        let signal = DummySignal;
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            101.0,
            99.0,
            100.5,
            1000000.0,
        );
        let intent = signal.generate(&[bar]);
        assert_eq!(intent, SignalIntent::Flat);
    }
}
