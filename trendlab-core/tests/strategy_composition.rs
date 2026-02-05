//! Integration tests for M7 - Strategy Composition
//!
//! These tests verify the complete strategy composition pipeline:
//! Signal → OrderPolicy → Sizer → Composer

use chrono::Utc;
use trendlab_core::{
    composer::StrategyComposer,
    domain::{Bar, Position},
    execution::Optimistic,
    order_policy::{ImmediateOrderPolicy, NaturalOrderPolicy, OrderPolicy},
    position_management::{OrderIntent, PositionManager},
    signals::{
        examples::{DonchianBreakout, MovingAverageCross},
        Signal, SignalFamily, SignalIntent,
    },
    sizers::{AtrRiskSizer, FixedSizer, Sizer},
};

/// Dummy position manager for testing
#[derive(Clone)]
struct NoOpPM;

impl PositionManager for NoOpPM {
    fn update(&mut self, _position: &Position, _bar: &Bar) -> Vec<OrderIntent> {
        vec![]
    }

    fn name(&self) -> &str {
        "NoOp"
    }

    fn clone_box(&self) -> Box<dyn PositionManager> {
        Box::new(self.clone())
    }
}

fn make_bar(timestamp_offset: i64, high: f64, low: f64, close: f64) -> Bar {
    use chrono::Duration;
    Bar::new(
        Utc::now() + Duration::seconds(timestamp_offset),
        "SPY".into(),
        (high + low) / 2.0,
        high,
        low,
        close,
        1000000.0,
    )
}

#[test]
fn test_signal_ignores_portfolio_state() {
    // Verify that signals are portfolio-agnostic:
    // Same bars → Same signal, regardless of current position

    let signal = DonchianBreakout::new(3);

    let bars = vec![
        make_bar(0, 102.0, 98.0, 100.0),
        make_bar(1, 103.0, 99.0, 101.0),
        make_bar(2, 104.0, 100.0, 102.0),
        make_bar(3, 110.0, 105.0, 108.0), // Breakout above 104.0
    ];

    // Signal should emit Long intent
    let intent1 = signal.generate(&bars);
    assert_eq!(intent1, SignalIntent::Long);

    // Even if we "pretend" we already have a position,
    // the signal should emit the SAME intent
    // (Signals don't see portfolio state)
    let intent2 = signal.generate(&bars);
    assert_eq!(intent2, SignalIntent::Long);

    // This demonstrates portfolio-agnosticism
    assert_eq!(intent1, intent2);
}

#[test]
fn test_breakout_signal_uses_stop_entry() {
    // Verify that breakout signals are naturally matched
    // with stop entries (not market entries)

    let policy = NaturalOrderPolicy::new(SignalFamily::Breakout, 100.0);
    let bar = make_bar(0, 105.0, 95.0, 100.0);

    // Breakout signal wants Long exposure
    let orders = policy.translate(SignalIntent::Long, None, &bar);

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].side, trendlab_core::domain::OrderSide::Buy);

    // Breakout family should use StopMarket entry
    assert!(matches!(
        orders[0].order_type,
        trendlab_core::domain::OrderType::StopMarket { .. }
    ));
}

#[test]
fn test_mean_reversion_signal_uses_limit_entry() {
    // Verify that mean-reversion signals are naturally matched
    // with limit entries (not stop entries)

    let policy = NaturalOrderPolicy::new(SignalFamily::MeanReversion, 100.0);
    let bar = make_bar(0, 105.0, 95.0, 100.0);

    let orders = policy.translate(SignalIntent::Long, None, &bar);

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].side, trendlab_core::domain::OrderSide::Buy);

    // Mean-reversion family should use Limit entry
    assert!(matches!(
        orders[0].order_type,
        trendlab_core::domain::OrderType::Limit { .. }
    ));
}

#[test]
fn test_trend_signal_uses_market_entry() {
    // Verify that trend signals use immediate market entries

    let policy = NaturalOrderPolicy::new(SignalFamily::Trend, 100.0);
    let bar = make_bar(0, 105.0, 95.0, 100.0);

    let orders = policy.translate(SignalIntent::Long, None, &bar);

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].side, trendlab_core::domain::OrderSide::Buy);

    // Trend family should use Market entry
    assert_eq!(
        orders[0].order_type,
        trendlab_core::domain::OrderType::Market
    );
}

#[test]
fn test_sizer_uses_equity_but_not_signal_logic() {
    // Verify that sizers use equity context but don't
    // change sizing based on which signal emitted the intent

    let sizer = FixedSizer::shares(100.0);
    let bar = make_bar(0, 105.0, 95.0, 100.0);

    // Same sizer, same equity, same intent → same quantity
    let qty1 = sizer.size(10000.0, SignalIntent::Long, &bar);
    let qty2 = sizer.size(10000.0, SignalIntent::Long, &bar);

    assert_eq!(qty1, 100.0);
    assert_eq!(qty2, 100.0);

    // Sizer doesn't care which signal generated the intent
}

#[test]
fn test_atr_risk_sizer_scales_with_volatility() {
    // Verify ATR-based risk sizing adjusts quantity based on volatility

    let sizer = AtrRiskSizer::new(0.01, 2.0, 3);

    // Low volatility bar → larger position
    let bar_low_vol = make_bar(0, 101.0, 99.0, 100.0); // Range = 2.0
    let qty_low_vol = sizer.size(100000.0, SignalIntent::Long, &bar_low_vol);

    // High volatility bar → smaller position
    let bar_high_vol = make_bar(0, 110.0, 90.0, 100.0); // Range = 20.0
    let qty_high_vol = sizer.size(100000.0, SignalIntent::Long, &bar_high_vol);

    // Higher volatility should result in smaller position
    assert!(qty_high_vol < qty_low_vol);
}

#[test]
fn test_full_strategy_composition() {
    // Integration test: compose a complete strategy and verify
    // all components are accessible

    let composer = StrategyComposer::new(
        Box::new(MovingAverageCross::new(20, 50)),
        Box::new(ImmediateOrderPolicy::new(100.0)),
        Box::new(NoOpPM),
        Box::new(FixedSizer::shares(100.0)),
        Box::new(Optimistic),
    );

    // Verify all components are accessible
    assert_eq!(composer.signal().name(), "MA_Cross");
    assert_eq!(composer.order_policy().name(), "Immediate");
    assert_eq!(composer.pm().name(), "NoOp");
    assert_eq!(composer.sizer().name(), "FixedShares");
    assert_eq!(composer.execution_preset().name(), "Optimistic");

    // Verify manifest generation
    let manifest = composer.manifest();
    assert_eq!(manifest.signal_name, "MA_Cross");
    assert_eq!(manifest.order_policy_name, "Immediate");
    assert_eq!(manifest.pm_name, "NoOp");
    assert_eq!(manifest.sizer_name, "FixedShares");
    assert_eq!(manifest.execution_preset, "Optimistic");

    // Verify manifest hash is deterministic
    assert!(manifest.verify_hash());
}

#[test]
fn test_fair_comparison_same_pm_different_signals() {
    // Demonstrate fair comparison: two strategies with different signals
    // but identical PM/execution/sizing can be compared directly

    let strategy_a = StrategyComposer::new(
        Box::new(MovingAverageCross::new(20, 50)),
        Box::new(ImmediateOrderPolicy::new(100.0)),
        Box::new(NoOpPM),
        Box::new(FixedSizer::shares(100.0)),
        Box::new(Optimistic),
    );

    let strategy_b = StrategyComposer::new(
        Box::new(DonchianBreakout::new(20)),
        Box::new(ImmediateOrderPolicy::new(100.0)),
        Box::new(NoOpPM),
        Box::new(FixedSizer::shares(100.0)),
        Box::new(Optimistic),
    );

    // Different signals
    assert_ne!(strategy_a.signal().name(), strategy_b.signal().name());

    // Same PM, sizer, execution
    assert_eq!(strategy_a.pm().name(), strategy_b.pm().name());
    assert_eq!(strategy_a.sizer().name(), strategy_b.sizer().name());
    assert_eq!(
        strategy_a.execution_preset().name(),
        strategy_b.execution_preset().name()
    );

    // Manifests should differ only in signal name
    let manifest_a = strategy_a.manifest();
    let manifest_b = strategy_b.manifest();

    assert_ne!(manifest_a.signal_name, manifest_b.signal_name);
    assert_eq!(manifest_a.pm_name, manifest_b.pm_name);
    assert_eq!(manifest_a.sizer_name, manifest_b.sizer_name);
    assert_eq!(manifest_a.execution_preset, manifest_b.execution_preset);

    // Different manifests → different hashes
    assert_ne!(manifest_a.config_hash, manifest_b.config_hash);
}

#[test]
fn test_manifest_hash_stability() {
    // Verify that identical strategies produce identical manifest hashes
    // (important for caching and reproducibility)

    let strategy_1 = StrategyComposer::new(
        Box::new(DonchianBreakout::new(20)),
        Box::new(NaturalOrderPolicy::new(SignalFamily::Breakout, 100.0)),
        Box::new(NoOpPM),
        Box::new(FixedSizer::notional(10000.0)),
        Box::new(Optimistic),
    );

    let strategy_2 = StrategyComposer::new(
        Box::new(DonchianBreakout::new(20)),
        Box::new(NaturalOrderPolicy::new(SignalFamily::Breakout, 100.0)),
        Box::new(NoOpPM),
        Box::new(FixedSizer::notional(10000.0)),
        Box::new(Optimistic),
    );

    let manifest_1 = strategy_1.manifest();
    let manifest_2 = strategy_2.manifest();

    // Identical strategies → identical hashes
    assert_eq!(manifest_1.config_hash, manifest_2.config_hash);
}
