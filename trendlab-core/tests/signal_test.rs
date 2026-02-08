//! Integration tests for all 10 signal generators.
//!
//! Tests:
//! 1. Each signal produces non-empty output on 252 bars of synthetic data.
//! 2. Look-ahead contamination: truncated series and full series produce identical
//!    signals for overlapping bars.
//! 3. NaN injection: no signal fires on NaN bars; signals still fire on valid bars.
//! 4. Portfolio agnosticism: compile-time invariant (comment-only).

use chrono::NaiveDate;
use std::collections::BTreeMap;
use trendlab_core::components::factory::{create_signal, required_indicators};
use trendlab_core::components::indicator::{Indicator, IndicatorValues};
use trendlab_core::components::signal::SignalGenerator;
use trendlab_core::domain::Bar;
use trendlab_core::fingerprint::ComponentConfig;

// ──────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────

fn btree(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Build a `ComponentConfig` from a type name and param pairs.
fn config(component_type: &str, params: &[(&str, f64)]) -> ComponentConfig {
    ComponentConfig {
        component_type: component_type.to_string(),
        params: btree(params),
    }
}

/// No-filter config stub for `required_indicators`.
fn no_filter_config() -> ComponentConfig {
    ComponentConfig {
        component_type: "no_filter".to_string(),
        params: BTreeMap::new(),
    }
}

/// No-op PM config stub for `required_indicators`.
fn no_op_pm_config() -> ComponentConfig {
    ComponentConfig {
        component_type: "no_op".to_string(),
        params: BTreeMap::new(),
    }
}

/// Signals that use Donchian-upper indicators.
///
/// The Donchian indicator includes the current bar's high in its window, so
/// `close <= high <= donchian_upper` always holds. These signals can only fire
/// when indicator values are lagged by one bar (simulating T-1 decision data).
/// The test uses a shifted Donchian series for these signals.
const DONCHIAN_SIGNALS: &[&str] = &["breakout_52w", "donchian_breakout"];

/// All 10 signal configs with short-enough lookback periods to fire within 252 bars.
fn signal_configs() -> Vec<(&'static str, ComponentConfig)> {
    vec![
        (
            "breakout_52w",
            config(
                "breakout_52w",
                &[("lookback", 50.0), ("threshold_pct", 0.0)],
            ),
        ),
        (
            "donchian_breakout",
            config("donchian_breakout", &[("entry_lookback", 20.0)]),
        ),
        (
            "bollinger_breakout",
            config(
                "bollinger_breakout",
                &[("period", 20.0), ("std_multiplier", 2.0)],
            ),
        ),
        (
            "keltner_breakout",
            config(
                "keltner_breakout",
                &[
                    ("ema_period", 20.0),
                    ("atr_period", 10.0),
                    ("multiplier", 1.5),
                ],
            ),
        ),
        (
            "supertrend",
            config("supertrend", &[("period", 10.0), ("multiplier", 3.0)]),
        ),
        (
            "parabolic_sar",
            config(
                "parabolic_sar",
                &[("af_start", 0.02), ("af_step", 0.02), ("af_max", 0.20)],
            ),
        ),
        (
            "ma_crossover",
            config(
                "ma_crossover",
                &[
                    ("fast_period", 5.0),
                    ("slow_period", 20.0),
                    ("ma_type", 0.0),
                ],
            ),
        ),
        ("tsmom", config("tsmom", &[("lookback", 10.0)])),
        (
            "roc_momentum",
            config("roc_momentum", &[("period", 10.0), ("threshold_pct", 0.0)]),
        ),
        (
            "aroon_crossover",
            config("aroon_crossover", &[("period", 10.0)]),
        ),
    ]
}

/// Generate `n` bars of synthetic data with a V-shape pattern.
///
/// The series starts with a downtrend (bars 0 to n/3), then transitions to
/// a strong uptrend (bars n/3 to end). This ensures that:
/// - Crossover/flip signals fire at the inflection point
/// - Breakout signals fire as price exceeds prior highs on the way up
/// - Momentum signals fire due to clear directional moves
fn trending_bars(n: usize) -> Vec<Bar> {
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let inflection = n / 3;
    (0..n)
        .map(|i| {
            let close = if i <= inflection {
                // Downtrend: drop from 200 to ~130
                200.0 - (i as f64) * 1.0
            } else {
                // Uptrend: steep slope of 3.0 per bar (exceeds high-close spread
                // of 1.5 so breakout signals can fire with lagged donchian).
                let bars_since = (i - inflection) as f64;
                200.0 - (inflection as f64) * 1.0 + bars_since * 3.0
            };
            Bar {
                symbol: "TEST".to_string(),
                date: base_date + chrono::Duration::days(i as i64),
                open: close - 0.5,
                high: close + 1.5,
                low: close - 1.5,
                close,
                volume: 1000,
                adj_close: close,
            }
        })
        .collect()
}

/// Compute all required indicators for a set of bars and return `IndicatorValues`.
///
/// For Donchian-upper indicators, the values are shifted forward by one bar
/// (the value at bar i becomes the value at bar i+1). This simulates using
/// bar T-1 data at decision time T, which is the only way the breakout_52w
/// and donchian_breakout signals can fire — the Donchian indicator includes
/// the current bar's high in its window, so `close <= donchian_upper` always
/// holds if the current bar is included. The one-bar lag makes the previous
/// bar's channel the breakout reference.
fn compute_indicators_for_signal(
    bars: &[Bar],
    indicators: &[Box<dyn Indicator>],
    signal_name: &str,
) -> IndicatorValues {
    let mut iv = IndicatorValues::new();
    let needs_lag = DONCHIAN_SIGNALS.contains(&signal_name);

    for ind in indicators {
        let values = ind.compute(bars);
        let final_values = if needs_lag && ind.name().starts_with("donchian_upper") {
            // Shift values forward by one: value[i] = original[i-1].
            // This means bar i gets the donchian upper from bar i-1,
            // which does not include bar i's high.
            let mut lagged = vec![f64::NAN; values.len()];
            for j in 1..values.len() {
                lagged[j] = values[j - 1];
            }
            lagged
        } else {
            values
        };
        iv.insert(ind.name().to_string(), final_values);
    }
    iv
}

/// Standard indicator computation (no lag), used for look-ahead tests
/// where we need identical computation on both truncated and full series.
fn compute_indicators(bars: &[Bar], indicators: &[Box<dyn Indicator>]) -> IndicatorValues {
    let mut iv = IndicatorValues::new();
    for ind in indicators {
        let values = ind.compute(bars);
        iv.insert(ind.name().to_string(), values);
    }
    iv
}

/// Evaluate a signal on every bar and return the indices where it fires.
fn evaluate_all(
    signal: &dyn SignalGenerator,
    bars: &[Bar],
    indicators: &IndicatorValues,
) -> Vec<usize> {
    (0..bars.len())
        .filter(|&i| signal.evaluate(bars, i, indicators).is_some())
        .collect()
}

/// Evaluate a signal on every bar and return `Option<bool>` per bar:
/// `None` = no signal, `Some(true)` = Long, `Some(false)` = Short.
/// Used for look-ahead contamination comparison.
fn evaluate_pattern(
    signal: &dyn SignalGenerator,
    bars: &[Bar],
    indicators: &IndicatorValues,
) -> Vec<Option<bool>> {
    (0..bars.len())
        .map(|i| {
            signal.evaluate(bars, i, indicators).map(|evt| {
                evt.direction == trendlab_core::components::signal::SignalDirection::Long
            })
        })
        .collect()
}

// ──────────────────────────────────────────────
// 1. Non-empty output on 252 bars
// ──────────────────────────────────────────────

#[test]
fn each_signal_fires_at_least_once_on_252_bars() {
    let bars = trending_bars(252);

    for (name, cfg) in signal_configs() {
        let signal =
            create_signal(&cfg).unwrap_or_else(|e| panic!("factory failed for {name}: {e}"));
        let indicators = required_indicators(&cfg, &no_filter_config(), &no_op_pm_config());
        let iv = compute_indicators_for_signal(&bars, &indicators, name);

        let fire_count = evaluate_all(signal.as_ref(), &bars, &iv).len();
        assert!(
            fire_count > 0,
            "signal '{name}' did not fire on 252 trending bars (fire_count = 0)"
        );
    }
}

// ──────────────────────────────────────────────
// 2. Look-ahead contamination test
// ──────────────────────────────────────────────

#[test]
fn no_look_ahead_contamination() {
    let full_bars = trending_bars(200);
    let truncated_bars: Vec<Bar> = full_bars[..100].to_vec();

    for (name, cfg) in signal_configs() {
        let signal =
            create_signal(&cfg).unwrap_or_else(|e| panic!("factory failed for {name}: {e}"));
        let indicators = required_indicators(&cfg, &no_filter_config(), &no_op_pm_config());

        // Compute on truncated series (standard computation — no lag needed
        // for look-ahead test since we compare two identical computations).
        let iv_truncated = compute_indicators(&truncated_bars, &indicators);
        let pattern_truncated = evaluate_pattern(signal.as_ref(), &truncated_bars, &iv_truncated);

        // Compute on full series.
        let iv_full = compute_indicators(&full_bars, &indicators);
        let pattern_full = evaluate_pattern(signal.as_ref(), &full_bars, &iv_full);

        // Compare bars 0..100: must be identical.
        for i in 0..100 {
            assert_eq!(
                pattern_truncated[i], pattern_full[i],
                "look-ahead contamination in signal '{name}' at bar {i}: \
                 truncated={:?}, full={:?}",
                pattern_truncated[i], pattern_full[i],
            );
        }
    }
}

// ──────────────────────────────────────────────
// 3. NaN injection test
// ──────────────────────────────────────────────

#[test]
fn nan_bars_produce_no_signal() {
    // Place NaN bars at indices 200, 201, 202 -- well past all warmup periods
    // (max warmup is ~50 for breakout_52w) so signals have room to fire on
    // valid bars before the NaN injection point.
    //
    // Note: EMA-based indicators (Keltner, Supertrend) permanently propagate
    // NaN, so all post-NaN bars may have NaN indicator values. The test
    // therefore checks that signals fire on valid bars *before* the NaN point,
    // rather than requiring recovery after the NaN.
    let mut bars = trending_bars(252);
    let nan_indices: Vec<usize> = vec![200, 201, 202];

    for &i in &nan_indices {
        bars[i].open = f64::NAN;
        bars[i].high = f64::NAN;
        bars[i].low = f64::NAN;
        bars[i].close = f64::NAN;
        bars[i].adj_close = f64::NAN;
    }

    for (name, cfg) in signal_configs() {
        let signal =
            create_signal(&cfg).unwrap_or_else(|e| panic!("factory failed for {name}: {e}"));
        let indicators = required_indicators(&cfg, &no_filter_config(), &no_op_pm_config());
        let iv = compute_indicators_for_signal(&bars, &indicators, name);

        // No signal on the NaN bars themselves.
        for &nan_idx in &nan_indices {
            let result = signal.evaluate(&bars, nan_idx, &iv);
            assert!(
                result.is_none(),
                "signal '{name}' fired on NaN bar at index {nan_idx}"
            );
        }

        // Signal should fire on at least one valid bar before the NaN injection.
        let fire_indices = evaluate_all(signal.as_ref(), &bars, &iv);
        let pre_nan_fires: Vec<usize> = fire_indices
            .into_iter()
            .filter(|&idx| idx < nan_indices[0])
            .collect();
        assert!(
            !pre_nan_fires.is_empty(),
            "signal '{name}' did not fire on any bar before NaN injection at index {}",
            nan_indices[0]
        );
    }
}

// ──────────────────────────────────────────────
// 4. Portfolio agnosticism (compile-time)
// ──────────────────────────────────────────────

/// Portfolio agnosticism is enforced by the trait signature:
///
/// ```rust,ignore
/// fn evaluate(
///     &self,
///     bars: &[Bar],
///     bar_index: usize,
///     indicators: &IndicatorValues,
/// ) -> Option<SignalEvent>;
/// ```
///
/// There is no `Portfolio`, `Position`, or any portfolio-state parameter.
/// If someone adds one, every signal implementation breaks at compile time.
/// This test exists purely to document the invariant — no runtime assertion
/// is needed because the type system enforces it.
#[test]
fn portfolio_agnosticism_is_enforced_by_trait_signature() {
    // The SignalGenerator::evaluate method takes only:
    //   &self, bars: &[Bar], bar_index: usize, indicators: &IndicatorValues
    // No portfolio, no position, no account state.
    //
    // This is a compile-time invariant. If the trait signature changes to
    // include portfolio state, this test file will fail to compile because
    // all the evaluate() calls above would have the wrong number of arguments.
    //
    // See also: trendlab-core/src/lib.rs::signal_generator_trait_has_no_portfolio_parameter
}

// ──────────────────────────────────────────────
// 5. Per-signal smoke tests (verbose names)
// ──────────────────────────────────────────────
// These are individual named tests so CI output shows exactly which signal
// failed, rather than a single parametric test.

macro_rules! signal_smoke_test {
    ($test_name:ident, $signal_name:expr, $signal_type:expr, $params:expr) => {
        #[test]
        fn $test_name() {
            let cfg = config($signal_type, $params);
            let signal = create_signal(&cfg)
                .unwrap_or_else(|e| panic!("factory failed for {}: {e}", $signal_type));
            let bars = trending_bars(252);
            let indicators = required_indicators(&cfg, &no_filter_config(), &no_op_pm_config());
            let iv = compute_indicators_for_signal(&bars, &indicators, $signal_name);

            let fires = evaluate_all(signal.as_ref(), &bars, &iv);
            assert!(
                !fires.is_empty(),
                "{} did not fire on 252 trending bars",
                $signal_type
            );

            // Verify signal name is non-empty.
            let sig_name = signal.name();
            assert!(!sig_name.is_empty(), "{} has empty name", $signal_type);
        }
    };
}

signal_smoke_test!(
    smoke_breakout_52w,
    "breakout_52w",
    "breakout_52w",
    &[("lookback", 50.0), ("threshold_pct", 0.0)]
);

signal_smoke_test!(
    smoke_donchian_breakout,
    "donchian_breakout",
    "donchian_breakout",
    &[("entry_lookback", 20.0)]
);

signal_smoke_test!(
    smoke_bollinger_breakout,
    "bollinger_breakout",
    "bollinger_breakout",
    &[("period", 20.0), ("std_multiplier", 2.0)]
);

signal_smoke_test!(
    smoke_keltner_breakout,
    "keltner_breakout",
    "keltner_breakout",
    &[
        ("ema_period", 20.0),
        ("atr_period", 10.0),
        ("multiplier", 1.5)
    ]
);

signal_smoke_test!(
    smoke_supertrend,
    "supertrend",
    "supertrend",
    &[("period", 10.0), ("multiplier", 3.0)]
);

signal_smoke_test!(
    smoke_parabolic_sar,
    "parabolic_sar",
    "parabolic_sar",
    &[("af_start", 0.02), ("af_step", 0.02), ("af_max", 0.20)]
);

signal_smoke_test!(
    smoke_ma_crossover,
    "ma_crossover",
    "ma_crossover",
    &[
        ("fast_period", 5.0),
        ("slow_period", 20.0),
        ("ma_type", 0.0)
    ]
);

signal_smoke_test!(smoke_tsmom, "tsmom", "tsmom", &[("lookback", 10.0)]);

signal_smoke_test!(
    smoke_roc_momentum,
    "roc_momentum",
    "roc_momentum",
    &[("period", 10.0), ("threshold_pct", 0.0)]
);

signal_smoke_test!(
    smoke_aroon_crossover,
    "aroon_crossover",
    "aroon_crossover",
    &[("period", 10.0)]
);
