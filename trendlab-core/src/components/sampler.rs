//! YOLO random sampler — generates random strategy compositions.
//!
//! Two controls:
//! - `jitter_pct` (0.0 to 1.0): how much to randomize parameter values
//! - `structural_explore` (0.0 to 1.0): probability of picking non-default component types

use rand::Rng;
use std::collections::BTreeMap;

use crate::fingerprint::{ComponentConfig, StrategyConfig};

/// Range for a numeric parameter.
#[derive(Debug, Clone)]
pub struct ParamRange {
    pub name: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

/// A component variant with its parameter ranges.
#[derive(Debug, Clone)]
pub struct ComponentVariant {
    pub component_type: String,
    pub param_ranges: Vec<ParamRange>,
    /// Weight for selection (higher = more likely to be picked).
    pub weight: f64,
}

/// Pool of all component variants for random sampling.
#[derive(Debug, Clone)]
pub struct ComponentPool {
    pub signals: Vec<ComponentVariant>,
    pub position_managers: Vec<ComponentVariant>,
    pub execution_models: Vec<ComponentVariant>,
    pub filters: Vec<ComponentVariant>,
}

impl ComponentPool {
    /// Default pool with all 10 signals, 9 PMs, 4 executions, 4 filters.
    pub fn default_pool() -> Self {
        Self {
            signals: vec![
                ComponentVariant {
                    component_type: "donchian_breakout".into(),
                    param_ranges: vec![ParamRange {
                        name: "entry_lookback".into(),
                        default: 50.0,
                        min: 10.0,
                        max: 200.0,
                    }],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "bollinger_breakout".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "period".into(),
                            default: 20.0,
                            min: 10.0,
                            max: 50.0,
                        },
                        ParamRange {
                            name: "std_multiplier".into(),
                            default: 2.0,
                            min: 1.0,
                            max: 3.0,
                        },
                    ],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "breakout_52w".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "lookback".into(),
                            default: 252.0,
                            min: 50.0,
                            max: 504.0,
                        },
                        ParamRange {
                            name: "threshold_pct".into(),
                            default: 0.0,
                            min: 0.0,
                            max: 5.0,
                        },
                    ],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "keltner_breakout".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "ema_period".into(),
                            default: 20.0,
                            min: 10.0,
                            max: 50.0,
                        },
                        ParamRange {
                            name: "atr_period".into(),
                            default: 10.0,
                            min: 5.0,
                            max: 30.0,
                        },
                        ParamRange {
                            name: "multiplier".into(),
                            default: 1.5,
                            min: 0.5,
                            max: 3.0,
                        },
                    ],
                    weight: 1.5,
                },
                ComponentVariant {
                    component_type: "supertrend".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "period".into(),
                            default: 10.0,
                            min: 5.0,
                            max: 30.0,
                        },
                        ParamRange {
                            name: "multiplier".into(),
                            default: 3.0,
                            min: 1.0,
                            max: 5.0,
                        },
                    ],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "parabolic_sar".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "af_start".into(),
                            default: 0.02,
                            min: 0.01,
                            max: 0.05,
                        },
                        ParamRange {
                            name: "af_step".into(),
                            default: 0.02,
                            min: 0.01,
                            max: 0.05,
                        },
                        ParamRange {
                            name: "af_max".into(),
                            default: 0.20,
                            min: 0.10,
                            max: 0.40,
                        },
                    ],
                    weight: 1.5,
                },
                ComponentVariant {
                    component_type: "ma_crossover".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "fast_period".into(),
                            default: 10.0,
                            min: 5.0,
                            max: 30.0,
                        },
                        ParamRange {
                            name: "slow_period".into(),
                            default: 50.0,
                            min: 20.0,
                            max: 200.0,
                        },
                        ParamRange {
                            name: "ma_type".into(),
                            default: 0.0,
                            min: 0.0,
                            max: 1.0,
                        },
                    ],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "tsmom".into(),
                    param_ranges: vec![ParamRange {
                        name: "lookback".into(),
                        default: 20.0,
                        min: 5.0,
                        max: 60.0,
                    }],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "roc_momentum".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "period".into(),
                            default: 12.0,
                            min: 5.0,
                            max: 30.0,
                        },
                        ParamRange {
                            name: "threshold_pct".into(),
                            default: 0.0,
                            min: 0.0,
                            max: 5.0,
                        },
                    ],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "aroon_crossover".into(),
                    param_ranges: vec![ParamRange {
                        name: "period".into(),
                        default: 25.0,
                        min: 10.0,
                        max: 50.0,
                    }],
                    weight: 1.0,
                },
            ],
            position_managers: vec![
                ComponentVariant {
                    component_type: "atr_trailing".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "atr_period".into(),
                            default: 14.0,
                            min: 5.0,
                            max: 30.0,
                        },
                        ParamRange {
                            name: "multiplier".into(),
                            default: 3.0,
                            min: 1.0,
                            max: 5.0,
                        },
                    ],
                    weight: 3.0,
                },
                ComponentVariant {
                    component_type: "percent_trailing".into(),
                    param_ranges: vec![ParamRange {
                        name: "trail_pct".into(),
                        default: 0.05,
                        min: 0.01,
                        max: 0.15,
                    }],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "chandelier".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "atr_period".into(),
                            default: 22.0,
                            min: 10.0,
                            max: 30.0,
                        },
                        ParamRange {
                            name: "multiplier".into(),
                            default: 3.0,
                            min: 1.5,
                            max: 5.0,
                        },
                    ],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "fixed_stop_loss".into(),
                    param_ranges: vec![ParamRange {
                        name: "stop_pct".into(),
                        default: 0.02,
                        min: 0.005,
                        max: 0.10,
                    }],
                    weight: 1.5,
                },
                ComponentVariant {
                    component_type: "breakeven_then_trail".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "breakeven_trigger_pct".into(),
                            default: 0.02,
                            min: 0.005,
                            max: 0.05,
                        },
                        ParamRange {
                            name: "trail_pct".into(),
                            default: 0.03,
                            min: 0.01,
                            max: 0.10,
                        },
                    ],
                    weight: 1.5,
                },
                ComponentVariant {
                    component_type: "time_decay".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "initial_pct".into(),
                            default: 0.10,
                            min: 0.03,
                            max: 0.20,
                        },
                        ParamRange {
                            name: "decay_per_bar".into(),
                            default: 0.005,
                            min: 0.001,
                            max: 0.02,
                        },
                        ParamRange {
                            name: "min_pct".into(),
                            default: 0.02,
                            min: 0.005,
                            max: 0.05,
                        },
                    ],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "frozen_reference".into(),
                    param_ranges: vec![ParamRange {
                        name: "exit_pct".into(),
                        default: 0.05,
                        min: 0.01,
                        max: 0.15,
                    }],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "since_entry_trailing".into(),
                    param_ranges: vec![ParamRange {
                        name: "exit_pct".into(),
                        default: 0.05,
                        min: 0.01,
                        max: 0.15,
                    }],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "max_holding_period".into(),
                    param_ranges: vec![ParamRange {
                        name: "max_bars".into(),
                        default: 20.0,
                        min: 5.0,
                        max: 60.0,
                    }],
                    weight: 0.5,
                },
            ],
            execution_models: vec![
                ComponentVariant {
                    component_type: "next_bar_open".into(),
                    param_ranges: vec![ParamRange {
                        name: "preset".into(),
                        default: 1.0,
                        min: 0.0,
                        max: 3.0,
                    }],
                    weight: 3.0,
                },
                ComponentVariant {
                    component_type: "stop_entry".into(),
                    param_ranges: vec![ParamRange {
                        name: "preset".into(),
                        default: 1.0,
                        min: 0.0,
                        max: 3.0,
                    }],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "close_on_signal".into(),
                    param_ranges: vec![ParamRange {
                        name: "preset".into(),
                        default: 1.0,
                        min: 0.0,
                        max: 3.0,
                    }],
                    weight: 1.0,
                },
                ComponentVariant {
                    component_type: "limit_entry".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "preset".into(),
                            default: 1.0,
                            min: 0.0,
                            max: 3.0,
                        },
                        ParamRange {
                            name: "offset_bps".into(),
                            default: 25.0,
                            min: 5.0,
                            max: 100.0,
                        },
                    ],
                    weight: 1.0,
                },
            ],
            filters: vec![
                ComponentVariant {
                    component_type: "no_filter".into(),
                    param_ranges: vec![],
                    weight: 3.0,
                },
                ComponentVariant {
                    component_type: "adx_filter".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "period".into(),
                            default: 14.0,
                            min: 7.0,
                            max: 28.0,
                        },
                        ParamRange {
                            name: "threshold".into(),
                            default: 25.0,
                            min: 15.0,
                            max: 40.0,
                        },
                    ],
                    weight: 2.0,
                },
                ComponentVariant {
                    component_type: "ma_regime".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "period".into(),
                            default: 200.0,
                            min: 50.0,
                            max: 400.0,
                        },
                        ParamRange {
                            name: "direction".into(),
                            default: 0.0,
                            min: 0.0,
                            max: 1.0,
                        },
                    ],
                    weight: 1.5,
                },
                ComponentVariant {
                    component_type: "volatility_filter".into(),
                    param_ranges: vec![
                        ParamRange {
                            name: "period".into(),
                            default: 14.0,
                            min: 7.0,
                            max: 28.0,
                        },
                        ParamRange {
                            name: "min_pct".into(),
                            default: 0.5,
                            min: 0.1,
                            max: 2.0,
                        },
                        ParamRange {
                            name: "max_pct".into(),
                            default: 5.0,
                            min: 2.0,
                            max: 10.0,
                        },
                    ],
                    weight: 1.5,
                },
            ],
        }
    }
}

/// Sample a random StrategyConfig from the component pool.
///
/// - `jitter_pct` (0.0 to 1.0): how much parameters deviate from defaults.
///   0.0 = all defaults, 1.0 = full range random.
/// - `structural_explore` (0.0 to 1.0): probability of picking non-default
///   components. Uses a cubic schedule: `prob = structural_explore^3` so low
///   values stay near proven combos.
pub fn sample_composition<R: Rng>(
    pool: &ComponentPool,
    rng: &mut R,
    jitter_pct: f64,
    structural_explore: f64,
) -> StrategyConfig {
    let jitter = jitter_pct.clamp(0.0, 1.0);
    let explore = structural_explore.clamp(0.0, 1.0);
    let explore_prob = explore * explore * explore; // cubic schedule

    let signal = sample_component(rng, &pool.signals, jitter, explore_prob);
    let pm = sample_component(rng, &pool.position_managers, jitter, explore_prob);
    let execution = sample_component(rng, &pool.execution_models, jitter, explore_prob);
    let filter = sample_component(rng, &pool.filters, jitter, explore_prob);

    // For discrete params (ma_type, direction, preset), round to nearest integer
    let signal = round_discrete_params(signal, &["ma_type"]);
    let execution = round_discrete_params(execution, &["preset"]);
    let filter = round_discrete_params(filter, &["direction"]);

    // Enforce cross-parameter constraints that factories assert on
    let signal = fix_cross_param_constraints(signal);
    let pm = fix_cross_param_constraints(pm);
    let filter = fix_cross_param_constraints(filter);

    StrategyConfig {
        signal,
        position_manager: pm,
        execution_model: execution,
        signal_filter: filter,
    }
}

fn sample_component<R: Rng>(
    rng: &mut R,
    variants: &[ComponentVariant],
    jitter: f64,
    explore_prob: f64,
) -> ComponentConfig {
    // Choose variant: weighted selection if exploring, otherwise stick with highest-weight
    let variant = if rng.gen::<f64>() < explore_prob || variants.len() == 1 {
        weighted_select(rng, variants)
    } else {
        // Stick with highest-weight (most "default") variant
        &variants[0]
    };

    // Sample params with jitter
    let mut params = BTreeMap::new();
    for range in &variant.param_ranges {
        let value = if jitter < 1e-10 {
            range.default
        } else {
            let spread = range.max - range.min;
            let offset = rng.gen::<f64>() * spread * jitter;
            let base = range.default - spread * jitter / 2.0;
            (base + offset).clamp(range.min, range.max)
        };
        params.insert(range.name.clone(), value);
    }

    ComponentConfig {
        component_type: variant.component_type.clone(),
        params,
    }
}

fn weighted_select<'a, R: Rng>(
    rng: &mut R,
    variants: &'a [ComponentVariant],
) -> &'a ComponentVariant {
    let total_weight: f64 = variants.iter().map(|v| v.weight).sum();
    let mut pick = rng.gen::<f64>() * total_weight;
    for variant in variants {
        pick -= variant.weight;
        if pick <= 0.0 {
            return variant;
        }
    }
    variants.last().unwrap()
}

fn round_discrete_params(mut config: ComponentConfig, discrete_keys: &[&str]) -> ComponentConfig {
    for key in discrete_keys {
        if let Some(val) = config.params.get_mut(*key) {
            *val = val.round();
        }
    }
    config
}

/// Enforce cross-parameter constraints that factory constructors assert on.
///
/// - `ma_crossover`: slow_period must be > fast_period (swap if needed, add 1 gap)
/// - `volatility_filter`: max_pct must be >= min_pct (swap if needed)
/// - `time_decay`: min_pct must be < initial_pct (swap if needed, shrink min)
fn fix_cross_param_constraints(mut config: ComponentConfig) -> ComponentConfig {
    match config.component_type.as_str() {
        "ma_crossover" => {
            let fast = config.params.get("fast_period").copied().unwrap_or(10.0);
            let slow = config.params.get("slow_period").copied().unwrap_or(50.0);
            if slow <= fast {
                // Ensure slow > fast: put the smaller value as fast, larger+1 as slow
                let new_fast = slow.min(fast);
                let new_slow = slow.max(fast) + 1.0;
                config.params.insert("fast_period".into(), new_fast);
                config.params.insert("slow_period".into(), new_slow);
            }
        }
        "volatility_filter" => {
            let min_pct = config.params.get("min_pct").copied().unwrap_or(0.5);
            let max_pct = config.params.get("max_pct").copied().unwrap_or(5.0);
            if max_pct < min_pct {
                config.params.insert("min_pct".into(), max_pct);
                config.params.insert("max_pct".into(), min_pct);
            }
        }
        "time_decay" => {
            let initial = config.params.get("initial_pct").copied().unwrap_or(0.10);
            let min = config.params.get("min_pct").copied().unwrap_or(0.02);
            if min >= initial {
                // Shrink min to half of initial
                config.params.insert("min_pct".into(), initial * 0.5);
            }
        }
        _ => {}
    }
    config
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::factory::{create_execution, create_filter, create_pm, create_signal};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    // ── Reproducibility: same seed produces identical results ────

    #[test]
    fn same_seed_produces_identical_results() {
        let pool = ComponentPool::default_pool();
        let seed = 42u64;

        let mut rng1 = StdRng::seed_from_u64(seed);
        let mut rng2 = StdRng::seed_from_u64(seed);

        let config1 = sample_composition(&pool, &mut rng1, 0.5, 0.5);
        let config2 = sample_composition(&pool, &mut rng2, 0.5, 0.5);

        assert_eq!(
            config1, config2,
            "Same seed must produce identical StrategyConfig"
        );

        // Also verify hashes match
        assert_eq!(config1.full_hash(), config2.full_hash());
        assert_eq!(config1.config_hash(), config2.config_hash());
    }

    #[test]
    fn different_seeds_produce_different_results() {
        let pool = ComponentPool::default_pool();

        let mut rng1 = StdRng::seed_from_u64(1);
        let mut rng2 = StdRng::seed_from_u64(2);

        let config1 = sample_composition(&pool, &mut rng1, 1.0, 1.0);
        let config2 = sample_composition(&pool, &mut rng2, 1.0, 1.0);

        // With full jitter and full explore, different seeds should yield different configs
        assert_ne!(
            config1.full_hash(),
            config2.full_hash(),
            "Different seeds should produce different configs"
        );
    }

    // ── All 1000 samples at (1.0, 1.0) produce valid factory configs ─

    #[test]
    fn all_samples_pass_factory_validation() {
        let pool = ComponentPool::default_pool();
        let mut rng = StdRng::seed_from_u64(12345);

        for i in 0..1000 {
            let config = sample_composition(&pool, &mut rng, 1.0, 1.0);

            create_signal(&config.signal).unwrap_or_else(|e| {
                panic!("Sample {} signal factory failed: {}", i, e);
            });
            create_pm(&config.position_manager).unwrap_or_else(|e| {
                panic!("Sample {} PM factory failed: {}", i, e);
            });
            create_execution(&config.execution_model).unwrap_or_else(|e| {
                panic!("Sample {} execution factory failed: {}", i, e);
            });
            create_filter(&config.signal_filter).unwrap_or_else(|e| {
                panic!("Sample {} filter factory failed: {}", i, e);
            });
        }
    }

    // ── Zero jitter produces default params ─────────────────────

    #[test]
    fn zero_jitter_produces_default_params() {
        let pool = ComponentPool::default_pool();
        let mut rng = StdRng::seed_from_u64(99);

        // With zero jitter and zero explore, we always get the first (default) variant
        // with its default parameters.
        for _ in 0..50 {
            let config = sample_composition(&pool, &mut rng, 0.0, 0.0);

            // Signal should be the first variant (donchian_breakout, weight=2.0)
            assert_eq!(config.signal.component_type, "donchian_breakout");
            assert_eq!(
                config.signal.params.get("entry_lookback").copied(),
                Some(50.0),
                "Zero jitter must produce default entry_lookback=50"
            );

            // PM should be the first variant (atr_trailing, weight=3.0)
            assert_eq!(config.position_manager.component_type, "atr_trailing");
            assert_eq!(
                config.position_manager.params.get("atr_period").copied(),
                Some(14.0),
            );
            assert_eq!(
                config.position_manager.params.get("multiplier").copied(),
                Some(3.0),
            );

            // Execution should be the first variant (next_bar_open, weight=3.0)
            assert_eq!(config.execution_model.component_type, "next_bar_open");
            assert_eq!(
                config.execution_model.params.get("preset").copied(),
                Some(1.0),
            );

            // Filter should be the first variant (no_filter, weight=3.0)
            assert_eq!(config.signal_filter.component_type, "no_filter");
            assert!(config.signal_filter.params.is_empty());
        }
    }

    // ── Structural explore at 0.0 picks default variant most of the time ─

    #[test]
    fn zero_explore_picks_default_variant() {
        let pool = ComponentPool::default_pool();
        let mut rng = StdRng::seed_from_u64(777);

        let mut default_signal_count = 0u32;
        let total = 500;

        for _ in 0..total {
            let config = sample_composition(&pool, &mut rng, 0.5, 0.0);
            if config.signal.component_type == "donchian_breakout" {
                default_signal_count += 1;
            }
        }

        // With explore=0.0, the cubic schedule gives explore_prob=0.0,
        // so we should always pick the first (default) variant.
        assert_eq!(
            default_signal_count, total as u32,
            "With explore=0.0, all signals must be the default variant (donchian_breakout)"
        );
    }

    #[test]
    fn high_explore_picks_diverse_variants() {
        let pool = ComponentPool::default_pool();
        let mut rng = StdRng::seed_from_u64(888);

        let mut signal_types = std::collections::HashSet::new();
        for _ in 0..500 {
            let config = sample_composition(&pool, &mut rng, 0.5, 1.0);
            signal_types.insert(config.signal.component_type.clone());
        }

        // With explore=1.0, cubic schedule = 1.0, we always weighted-select.
        // Over 500 samples we should see at least 5 different signal types.
        assert!(
            signal_types.len() >= 5,
            "With explore=1.0 over 500 samples, expected >=5 signal types, got {}",
            signal_types.len(),
        );
    }

    // ── Parameter bounds are always respected ────────────────────

    #[test]
    fn params_stay_within_bounds() {
        let pool = ComponentPool::default_pool();
        let mut rng = StdRng::seed_from_u64(333);

        for _ in 0..1000 {
            let config = sample_composition(&pool, &mut rng, 1.0, 1.0);

            // Check signal params against pool definitions
            check_bounds(&config.signal, &pool.signals);
            check_bounds(&config.position_manager, &pool.position_managers);
            check_bounds(&config.execution_model, &pool.execution_models);
            check_bounds(&config.signal_filter, &pool.filters);
        }
    }

    fn check_bounds(config: &ComponentConfig, variants: &[ComponentVariant]) {
        let variant = variants
            .iter()
            .find(|v| v.component_type == config.component_type)
            .unwrap_or_else(|| {
                panic!(
                    "Component type '{}' not found in pool",
                    config.component_type
                )
            });

        for range in &variant.param_ranges {
            if let Some(&val) = config.params.get(&range.name) {
                assert!(
                    val >= range.min && val <= range.max,
                    "Param '{}' of '{}' is {} but must be in [{}, {}]",
                    range.name,
                    config.component_type,
                    val,
                    range.min,
                    range.max,
                );
            }
        }
    }

    // ── Discrete params are rounded ─────────────────────────────

    #[test]
    fn discrete_params_are_rounded() {
        let pool = ComponentPool::default_pool();
        let mut rng = StdRng::seed_from_u64(555);

        for _ in 0..200 {
            let config = sample_composition(&pool, &mut rng, 1.0, 1.0);

            // preset on execution model should be integer
            if let Some(&preset) = config.execution_model.params.get("preset") {
                assert_eq!(
                    preset,
                    preset.round(),
                    "Execution preset must be integer, got {}",
                    preset,
                );
            }

            // ma_type on signal (if ma_crossover)
            if config.signal.component_type == "ma_crossover" {
                if let Some(&ma_type) = config.signal.params.get("ma_type") {
                    assert_eq!(
                        ma_type,
                        ma_type.round(),
                        "ma_type must be integer, got {}",
                        ma_type,
                    );
                }
            }

            // direction on filter (if ma_regime)
            if config.signal_filter.component_type == "ma_regime" {
                if let Some(&direction) = config.signal_filter.params.get("direction") {
                    assert_eq!(
                        direction,
                        direction.round(),
                        "direction must be integer, got {}",
                        direction,
                    );
                }
            }
        }
    }

    // ── Default pool has correct counts ─────────────────────────

    #[test]
    fn default_pool_has_correct_variant_counts() {
        let pool = ComponentPool::default_pool();
        assert_eq!(pool.signals.len(), 10, "Expected 10 signals");
        assert_eq!(pool.position_managers.len(), 9, "Expected 9 PMs");
        assert_eq!(
            pool.execution_models.len(),
            4,
            "Expected 4 execution models"
        );
        assert_eq!(pool.filters.len(), 4, "Expected 4 filters");
    }

    // ── Weighted selection respects weights ──────────────────────

    #[test]
    fn weighted_select_respects_weights() {
        let variants = vec![
            ComponentVariant {
                component_type: "heavy".into(),
                param_ranges: vec![],
                weight: 100.0,
            },
            ComponentVariant {
                component_type: "light".into(),
                param_ranges: vec![],
                weight: 1.0,
            },
        ];

        let mut rng = StdRng::seed_from_u64(42);
        let mut heavy_count = 0u32;
        for _ in 0..1000 {
            let v = weighted_select(&mut rng, &variants);
            if v.component_type == "heavy" {
                heavy_count += 1;
            }
        }

        // heavy has 100x the weight, so it should be picked ~99% of the time
        assert!(
            heavy_count > 950,
            "Expected heavy to be picked >950/1000 times, got {}",
            heavy_count,
        );
    }
}
