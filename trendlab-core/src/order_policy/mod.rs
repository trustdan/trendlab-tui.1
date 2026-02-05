//! Order Policy — translates signal intent into order types
//!
//! OrderPolicy bridges the gap between portfolio-agnostic signals
//! and the actual orders submitted to the execution engine.
//!
//! # Design Philosophy
//! Different signal families benefit from different order types:
//! - **Breakout signals** → StopMarket entries (enter on continuation)
//! - **Mean-reversion signals** → Limit entries (enter on pullback)
//! - **Trend signals** → Market or adaptive entries
//!
//! This separation allows:
//! 1. Fair comparison across signals with identical execution assumptions
//! 2. Testing signal families with their "natural" order types
//! 3. Explicit control over execution realism

pub mod natural;
pub mod immediate;

pub use natural::NaturalOrderPolicy;
pub use immediate::ImmediateOrderPolicy;

use crate::domain::{Bar, Order, Position};
use crate::signals::SignalIntent;

/// Policy for translating signal intent into concrete orders
pub trait OrderPolicy: Send + Sync {
    /// Translate signal intent into orders
    ///
    /// # Arguments
    /// - `intent`: Signal's desired exposure (Long/Short/Flat)
    /// - `current_position`: Current position state (if any)
    /// - `bar`: Current bar data for price/volume context
    ///
    /// # Returns
    /// Vector of orders to execute (can be empty, single, or multiple for complex strategies)
    ///
    /// # Examples
    /// - Breakout signal with Long intent → StopMarket buy above breakout level
    /// - Mean-reversion with Long intent → Limit buy at lower band
    /// - Flat intent with existing position → Market exit order
    fn translate(
        &self,
        intent: SignalIntent,
        current_position: Option<&Position>,
        bar: &Bar,
    ) -> Vec<Order>;

    /// Policy name for manifest/logging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Bar;
    use chrono::Utc;

    struct DummyPolicy;

    impl OrderPolicy for DummyPolicy {
        fn translate(
            &self,
            _intent: SignalIntent,
            _current_position: Option<&Position>,
            _bar: &Bar,
        ) -> Vec<Order> {
            vec![]
        }

        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn test_order_policy_trait_compiles() {
        let policy = DummyPolicy;
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            101.0,
            99.0,
            100.5,
            1000000.0,
        );
        let orders = policy.translate(SignalIntent::Long, None, &bar);
        assert!(orders.is_empty());
    }
}
