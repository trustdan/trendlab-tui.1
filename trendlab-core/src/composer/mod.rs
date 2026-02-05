//! Strategy Composer — assembles all strategy components
//!
//! The composer binds together:
//! - Signal (what to trade)
//! - OrderPolicy (how to enter)
//! - Sizer (how much to trade)
//! - PositionManager (when to exit)
//! - ExecutionPreset (realism level)
//!
//! This separation enables fair comparison: swap signals while keeping PM/execution constant.

pub mod manifest;

pub use manifest::StrategyManifest;

use crate::execution::ExecutionPreset;
use crate::order_policy::OrderPolicy;
use crate::position_management::PositionManager;
use crate::signals::Signal;
use crate::sizers::Sizer;

/// Assembled strategy configuration
///
/// # Design Philosophy
/// The composer is immutable after construction and generates a deterministic
/// manifest hash for caching and reproducibility.
///
/// # Usage
/// ```ignore
/// let strategy = StrategyComposer::new(
///     Box::new(DonchianBreakout::new(20)),
///     Box::new(NaturalOrderPolicy::new(SignalFamily::Breakout, 100.0)),
///     Box::new(FixedPercentStop::new(0.02)),
///     Box::new(FixedSizer::shares(100.0)),
///     ExecutionPreset::WorstCase,
/// );
///
/// let manifest = strategy.manifest();
/// ```
pub struct StrategyComposer {
    signal: Box<dyn Signal>,
    order_policy: Box<dyn OrderPolicy>,
    pm: Box<dyn PositionManager>,
    sizer: Box<dyn Sizer>,
    execution_preset: Box<dyn ExecutionPreset>,
}

impl StrategyComposer {
    pub fn new(
        signal: Box<dyn Signal>,
        order_policy: Box<dyn OrderPolicy>,
        pm: Box<dyn PositionManager>,
        sizer: Box<dyn Sizer>,
        execution_preset: Box<dyn ExecutionPreset>,
    ) -> Self {
        Self {
            signal,
            order_policy,
            pm,
            sizer,
            execution_preset,
        }
    }

    /// Get strategy manifest for caching and logging
    pub fn manifest(&self) -> StrategyManifest {
        StrategyManifest::new(
            self.signal.name().to_string(),
            self.order_policy.name().to_string(),
            self.pm.name().to_string(),
            self.sizer.name().to_string(),
            self.execution_preset.name().to_string(),
        )
    }

    /// Access signal component
    pub fn signal(&self) -> &dyn Signal {
        &*self.signal
    }

    /// Access order policy component
    pub fn order_policy(&self) -> &dyn OrderPolicy {
        &*self.order_policy
    }

    /// Access position manager component
    pub fn pm(&self) -> &dyn PositionManager {
        &*self.pm
    }

    /// Access sizer component
    pub fn sizer(&self) -> &dyn Sizer {
        &*self.sizer
    }

    /// Access execution preset
    pub fn execution_preset(&self) -> &dyn ExecutionPreset {
        &*self.execution_preset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Bar, Position};
    use crate::execution::Optimistic;
    use crate::order_policy::ImmediateOrderPolicy;
    use crate::position_management::OrderIntent;
    use crate::signals::{SignalFamily, SignalIntent};
    use crate::sizers::FixedSizer;

    // Dummy implementations for testing
    struct DummySignal;
    impl Signal for DummySignal {
        fn generate(&self, _bars: &[Bar]) -> SignalIntent {
            SignalIntent::Long
        }
        fn name(&self) -> &str {
            "DummySignal"
        }
        fn max_lookback(&self) -> usize {
            0
        }
        fn signal_family(&self) -> SignalFamily {
            SignalFamily::Trend
        }
    }

    #[derive(Clone)]
    struct DummyPM;
    impl PositionManager for DummyPM {
        fn update(&mut self, _position: &Position, _bar: &Bar) -> Vec<OrderIntent> {
            vec![]
        }
        fn name(&self) -> &str {
            "DummyPM"
        }
        fn clone_box(&self) -> Box<dyn PositionManager> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn test_composer_assembles_components() {
        let composer = StrategyComposer::new(
            Box::new(DummySignal),
            Box::new(ImmediateOrderPolicy::new(100.0)),
            Box::new(DummyPM),
            Box::new(FixedSizer::shares(100.0)),
            Box::new(Optimistic),
        );

        assert_eq!(composer.signal().name(), "DummySignal");
        assert_eq!(composer.order_policy().name(), "Immediate");
        assert_eq!(composer.pm().name(), "DummyPM");
        assert_eq!(composer.sizer().name(), "FixedShares");
        assert_eq!(composer.execution_preset().name(), "Optimistic");
    }

    #[test]
    fn test_composer_generates_manifest() {
        let composer = StrategyComposer::new(
            Box::new(DummySignal),
            Box::new(ImmediateOrderPolicy::new(100.0)),
            Box::new(DummyPM),
            Box::new(FixedSizer::shares(100.0)),
            Box::new(Optimistic),
        );

        let manifest = composer.manifest();
        assert_eq!(manifest.signal_name, "DummySignal");
        assert_eq!(manifest.order_policy_name, "Immediate");
        assert_eq!(manifest.pm_name, "DummyPM");
        assert_eq!(manifest.sizer_name, "FixedShares");
    }
}
