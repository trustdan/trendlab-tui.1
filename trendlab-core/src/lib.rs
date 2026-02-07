//! TrendLab Core — engine, domain types, event loop, orders, execution, position management.
//!
//! This crate contains the heart of the backtesting engine:
//! - Domain types (bars, orders, fills, positions, trades, instruments)
//! - Bar-by-bar event loop with four phases per bar
//! - Order book state machine
//! - Execution engine with configurable path policies
//! - Position management with ratchet invariant
//! - Signal and indicator traits
//! - Four-component composition model (signal + PM + execution + filter)

pub mod components;
pub mod domain;
pub mod fingerprint;
pub mod rng;
pub mod schema;
pub mod smoke;

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check: all core domain types are Send + Sync.
    ///
    /// This prevents a painful retrofit when the TUI worker thread is introduced
    /// in Phase 12. If any type fails this check, the build breaks immediately.
    #[allow(dead_code)]
    fn assert_send_sync() {
        fn require_send<T: Send>() {}
        fn require_sync<T: Sync>() {}

        // Domain types
        require_send::<domain::Bar>();
        require_sync::<domain::Bar>();
        require_send::<domain::MarketStatus>();
        require_sync::<domain::MarketStatus>();
        require_send::<domain::Order>();
        require_sync::<domain::Order>();
        require_send::<domain::Fill>();
        require_sync::<domain::Fill>();
        require_send::<domain::Position>();
        require_sync::<domain::Position>();
        require_send::<domain::Portfolio>();
        require_sync::<domain::Portfolio>();
        require_send::<domain::TradeRecord>();
        require_sync::<domain::TradeRecord>();
        require_send::<domain::Instrument>();
        require_sync::<domain::Instrument>();

        // ID types
        require_send::<domain::OrderId>();
        require_sync::<domain::OrderId>();
        require_send::<domain::SignalEventId>();
        require_sync::<domain::SignalEventId>();
        require_send::<domain::ConfigHash>();
        require_sync::<domain::ConfigHash>();
        require_send::<domain::RunId>();
        require_sync::<domain::RunId>();

        // Component types
        require_send::<components::SignalEvent>();
        require_sync::<components::SignalEvent>();
        require_send::<components::SignalEvaluation>();
        require_sync::<components::SignalEvaluation>();
        require_send::<components::OrderIntent>();
        require_sync::<components::OrderIntent>();
        require_send::<components::IndicatorValues>();
        require_sync::<components::IndicatorValues>();
        require_send::<components::PathPolicy>();
        require_sync::<components::PathPolicy>();
        require_send::<components::ExecutionPreset>();
        require_sync::<components::ExecutionPreset>();

        // Fingerprint types
        require_send::<fingerprint::StrategyConfig>();
        require_sync::<fingerprint::StrategyConfig>();
        require_send::<fingerprint::RunFingerprint>();
        require_sync::<fingerprint::RunFingerprint>();

        // RNG
        require_send::<rng::RngHierarchy>();
        require_sync::<rng::RngHierarchy>();
    }

    /// Architecture contract: SignalGenerator trait does NOT accept Portfolio.
    ///
    /// This is enforced by the trait signature itself — `evaluate()` takes
    /// `&[Bar]`, `usize`, and `&IndicatorValues`, with no portfolio parameter.
    /// If someone adds a portfolio parameter, the trait changes and all
    /// implementations break. This test documents the contract explicitly.
    #[test]
    fn signal_generator_trait_has_no_portfolio_parameter() {
        // The trait signature is:
        //   fn evaluate(&self, bars: &[Bar], bar_index: usize, indicators: &IndicatorValues)
        //       -> Option<SignalEvent>;
        //
        // If this compiles, signals cannot see portfolio state.
        // There is no runtime assertion needed — the type system enforces it.
        //
        // This test exists to document the invariant and break loudly if the
        // trait signature is ever modified to include portfolio state.
        fn _check_trait_object_builds(
            sig: &dyn components::SignalGenerator,
            bars: &[domain::Bar],
            indicators: &components::IndicatorValues,
        ) -> Option<components::SignalEvent> {
            sig.evaluate(bars, 0, indicators)
        }
    }

    /// Architecture contract: SignalFilter trait does NOT accept Portfolio.
    #[test]
    fn signal_filter_trait_has_no_portfolio_parameter() {
        fn _check_trait_object_builds(
            filter: &dyn components::SignalFilter,
            signal: &components::SignalEvent,
            bars: &[domain::Bar],
            indicators: &components::IndicatorValues,
        ) -> components::SignalEvaluation {
            filter.evaluate(signal, bars, 0, indicators)
        }
    }
}
