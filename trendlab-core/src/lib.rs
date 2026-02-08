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
pub mod data;
pub mod domain;
pub mod engine;
pub mod fingerprint;
pub mod indicators;
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

        // Engine types
        require_send::<engine::EngineConfig>();
        require_sync::<engine::EngineConfig>();
        require_send::<engine::RunResult>();
        require_sync::<engine::RunResult>();
        require_send::<engine::OrderBook>();
        require_sync::<engine::OrderBook>();

        // Position manager concrete types
        require_send::<components::AtrTrailing>();
        require_sync::<components::AtrTrailing>();
        require_send::<components::Chandelier>();
        require_sync::<components::Chandelier>();
        require_send::<components::PercentTrailing>();
        require_sync::<components::PercentTrailing>();
        require_send::<components::SinceEntryTrailing>();
        require_sync::<components::SinceEntryTrailing>();
        require_send::<components::FrozenReference>();
        require_sync::<components::FrozenReference>();
        require_send::<components::TimeDecay>();
        require_sync::<components::TimeDecay>();
        require_send::<components::MaxHoldingPeriod>();
        require_sync::<components::MaxHoldingPeriod>();
        require_send::<components::FixedStopLoss>();
        require_sync::<components::FixedStopLoss>();
        require_send::<components::BreakevenThenTrail>();
        require_sync::<components::BreakevenThenTrail>();
        require_send::<components::NoOpPm>();
        require_sync::<components::NoOpPm>();

        // Signal concrete types
        require_send::<components::signal::Breakout52w>();
        require_sync::<components::signal::Breakout52w>();
        require_send::<components::signal::DonchianBreakout>();
        require_sync::<components::signal::DonchianBreakout>();
        require_send::<components::signal::BollingerBreakout>();
        require_sync::<components::signal::BollingerBreakout>();
        require_send::<components::signal::KeltnerBreakout>();
        require_sync::<components::signal::KeltnerBreakout>();
        require_send::<components::signal::SupertrendSignal>();
        require_sync::<components::signal::SupertrendSignal>();
        require_send::<components::signal::ParabolicSarSignal>();
        require_sync::<components::signal::ParabolicSarSignal>();
        require_send::<components::signal::MaCrossover>();
        require_sync::<components::signal::MaCrossover>();
        require_send::<components::signal::Tsmom>();
        require_sync::<components::signal::Tsmom>();
        require_send::<components::signal::RocMomentum>();
        require_sync::<components::signal::RocMomentum>();
        require_send::<components::signal::AroonCrossover>();
        require_sync::<components::signal::AroonCrossover>();
        require_send::<components::signal::NullSignal>();
        require_sync::<components::signal::NullSignal>();

        // Filter concrete types
        require_send::<components::filter::NoFilter>();
        require_sync::<components::filter::NoFilter>();
        require_send::<components::filter::AdxFilter>();
        require_sync::<components::filter::AdxFilter>();
        require_send::<components::filter::MaRegimeFilter>();
        require_sync::<components::filter::MaRegimeFilter>();
        require_send::<components::filter::VolatilityFilter>();
        require_sync::<components::filter::VolatilityFilter>();
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
