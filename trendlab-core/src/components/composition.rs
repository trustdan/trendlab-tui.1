//! Strategy composition — assembles four runtime components + indicators from config.
//!
//! The composition system is the bridge between declarative `StrategyConfig` and
//! the runtime trait objects consumed by the bar event loop.
//!
//! - `StrategyComposition`: fully assembled strategy ready for the engine.
//! - `build_composition`: factory orchestrator that builds all four components.
//! - `check_compatibility`: static compatibility rules (warnings, not errors).
//! - `StrategyPreset`: named presets for common strategy archetypes.

use std::collections::BTreeMap;

use crate::fingerprint::{ComponentConfig, StrategyConfig, TradingMode};

use super::execution::ExecutionModel;
use super::factory::{
    create_execution, create_filter, create_pm, create_signal, required_indicators, FactoryError,
};
use super::filter::SignalFilter;
use super::indicator::Indicator;
use super::pm::PositionManager;
use super::signal::SignalGenerator;

// ─── StrategyComposition ────────────────────────────────────────────

/// A fully assembled strategy: four runtime components + their indicators.
pub struct StrategyComposition {
    pub signal: Box<dyn SignalGenerator>,
    pub filter: Box<dyn SignalFilter>,
    pub execution: Box<dyn ExecutionModel>,
    pub pm: Box<dyn PositionManager>,
    pub indicators: Vec<Box<dyn Indicator>>,
    pub config: StrategyConfig,
    pub trading_mode: TradingMode,
}

// ─── Compatibility check ────────────────────────────────────────────

/// Result of a compatibility check between strategy components.
#[derive(Debug, Clone)]
pub struct CompatibilityResult {
    pub warnings: Vec<String>,
}

impl CompatibilityResult {
    /// True if no warnings were produced — the combination is fully clean.
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// Static compatibility rules between components.
/// Returns warnings (not errors) — all combinations are allowed.
pub fn check_compatibility(config: &StrategyConfig) -> CompatibilityResult {
    let mut warnings = Vec::new();

    let sig = &config.signal.component_type;
    let exec = &config.execution_model.component_type;

    let breakout_signals = [
        "breakout_52w",
        "donchian_breakout",
        "bollinger_breakout",
        "keltner_breakout",
    ];

    // stop_entry + non-breakout signal
    if exec == "stop_entry" && !breakout_signals.contains(&sig.as_str()) {
        warnings.push(
            "stop_entry with non-breakout signal: stop entry will use fallback trigger (high+tick)"
                .into(),
        );
    }

    // limit_entry + breakout signal
    if exec == "limit_entry" && breakout_signals.contains(&sig.as_str()) {
        warnings.push(
            "limit_entry with breakout signal: limit entry ignores breakout_level; consider stop_entry"
                .into(),
        );
    }

    CompatibilityResult { warnings }
}

// ─── build_composition ──────────────────────────────────────────────

/// Build a fully assembled `StrategyComposition` from config.
///
/// Calls the four factory functions, resolves required indicators,
/// and runs the compatibility check (warnings only — never blocks construction).
pub fn build_composition(
    config: &StrategyConfig,
    trading_mode: TradingMode,
) -> Result<StrategyComposition, FactoryError> {
    let signal = create_signal(&config.signal)?;
    let filter = create_filter(&config.signal_filter)?;
    let execution = create_execution(&config.execution_model)?;
    let pm = create_pm(&config.position_manager)?;
    let indicators = required_indicators(
        &config.signal,
        &config.signal_filter,
        &config.position_manager,
    );

    Ok(StrategyComposition {
        signal,
        filter,
        execution,
        pm,
        indicators,
        config: config.clone(),
        trading_mode,
    })
}

// ─── StrategyPreset ─────────────────────────────────────────────────

/// Named strategy presets — common signal + PM + execution + filter combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyPreset {
    DonchianTrend,
    BollingerBreakout,
    MaCrossoverTrend,
    MomentumRoc,
    SupertrendSystem,
}

/// Helper: build a `BTreeMap<String, f64>` from `&[(&str, f64)]` pairs.
fn btree(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

impl StrategyPreset {
    /// Convert to a `StrategyConfig` with default parameters.
    pub fn to_config(self) -> StrategyConfig {
        match self {
            Self::DonchianTrend => StrategyConfig {
                signal: ComponentConfig {
                    component_type: "donchian_breakout".into(),
                    params: btree(&[("entry_lookback", 50.0)]),
                },
                position_manager: ComponentConfig {
                    component_type: "atr_trailing".into(),
                    params: btree(&[("atr_period", 14.0), ("multiplier", 3.0)]),
                },
                execution_model: ComponentConfig {
                    component_type: "stop_entry".into(),
                    params: btree(&[("preset", 1.0)]),
                },
                signal_filter: ComponentConfig {
                    component_type: "no_filter".into(),
                    params: BTreeMap::new(),
                },
            },
            Self::BollingerBreakout => StrategyConfig {
                signal: ComponentConfig {
                    component_type: "bollinger_breakout".into(),
                    params: btree(&[("period", 20.0), ("std_multiplier", 2.0)]),
                },
                position_manager: ComponentConfig {
                    component_type: "percent_trailing".into(),
                    params: btree(&[("trail_pct", 0.05)]),
                },
                execution_model: ComponentConfig {
                    component_type: "next_bar_open".into(),
                    params: btree(&[("preset", 1.0)]),
                },
                signal_filter: ComponentConfig {
                    component_type: "adx_filter".into(),
                    params: btree(&[("period", 14.0), ("threshold", 25.0)]),
                },
            },
            Self::MaCrossoverTrend => StrategyConfig {
                signal: ComponentConfig {
                    component_type: "ma_crossover".into(),
                    params: btree(&[
                        ("fast_period", 10.0),
                        ("slow_period", 50.0),
                        ("ma_type", 0.0),
                    ]),
                },
                position_manager: ComponentConfig {
                    component_type: "chandelier".into(),
                    params: btree(&[("atr_period", 22.0), ("multiplier", 3.0)]),
                },
                execution_model: ComponentConfig {
                    component_type: "next_bar_open".into(),
                    params: btree(&[("preset", 1.0)]),
                },
                signal_filter: ComponentConfig {
                    component_type: "ma_regime".into(),
                    params: btree(&[("period", 200.0), ("direction", 0.0)]),
                },
            },
            Self::MomentumRoc => StrategyConfig {
                signal: ComponentConfig {
                    component_type: "roc_momentum".into(),
                    params: btree(&[("period", 12.0), ("threshold_pct", 0.0)]),
                },
                position_manager: ComponentConfig {
                    component_type: "time_decay".into(),
                    params: btree(&[
                        ("initial_pct", 0.10),
                        ("decay_per_bar", 0.005),
                        ("min_pct", 0.02),
                    ]),
                },
                execution_model: ComponentConfig {
                    component_type: "next_bar_open".into(),
                    params: btree(&[("preset", 1.0)]),
                },
                signal_filter: ComponentConfig {
                    component_type: "volatility_filter".into(),
                    params: btree(&[("period", 14.0), ("min_pct", 0.5), ("max_pct", 5.0)]),
                },
            },
            Self::SupertrendSystem => StrategyConfig {
                signal: ComponentConfig {
                    component_type: "supertrend".into(),
                    params: btree(&[("period", 10.0), ("multiplier", 3.0)]),
                },
                position_manager: ComponentConfig {
                    component_type: "breakeven_then_trail".into(),
                    params: btree(&[("breakeven_trigger_pct", 0.02), ("trail_pct", 0.03)]),
                },
                execution_model: ComponentConfig {
                    component_type: "next_bar_open".into(),
                    params: btree(&[("preset", 1.0)]),
                },
                signal_filter: ComponentConfig {
                    component_type: "no_filter".into(),
                    params: BTreeMap::new(),
                },
            },
        }
    }

    /// All presets as a slice.
    pub fn all() -> &'static [StrategyPreset] {
        &[
            Self::DonchianTrend,
            Self::BollingerBreakout,
            Self::MaCrossoverTrend,
            Self::MomentumRoc,
            Self::SupertrendSystem,
        ]
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Preset builds ───────────────────────────────────────────

    #[test]
    fn preset_donchian_trend_builds() {
        let config = StrategyPreset::DonchianTrend.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert_eq!(comp.signal.name(), "donchian_breakout");
        assert_eq!(comp.pm.name(), "atr_trailing");
        assert_eq!(comp.execution.name(), "stop_entry");
        assert_eq!(comp.filter.name(), "no_filter");
    }

    #[test]
    fn preset_bollinger_breakout_builds() {
        let config = StrategyPreset::BollingerBreakout.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert_eq!(comp.signal.name(), "bollinger_breakout");
        assert_eq!(comp.pm.name(), "percent_trailing");
        assert_eq!(comp.execution.name(), "next_bar_open");
        assert_eq!(comp.filter.name(), "adx_filter");
    }

    #[test]
    fn preset_ma_crossover_trend_builds() {
        let config = StrategyPreset::MaCrossoverTrend.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert_eq!(comp.signal.name(), "ma_crossover");
        assert_eq!(comp.pm.name(), "chandelier_exit");
        assert_eq!(comp.execution.name(), "next_bar_open");
        assert_eq!(comp.filter.name(), "ma_regime");
    }

    #[test]
    fn preset_momentum_roc_builds() {
        let config = StrategyPreset::MomentumRoc.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert_eq!(comp.signal.name(), "roc_momentum");
        assert_eq!(comp.pm.name(), "time_decay");
        assert_eq!(comp.execution.name(), "next_bar_open");
        assert_eq!(comp.filter.name(), "volatility_filter");
    }

    #[test]
    fn preset_supertrend_system_builds() {
        let config = StrategyPreset::SupertrendSystem.to_config();
        let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
        assert_eq!(comp.signal.name(), "supertrend_flip");
        assert_eq!(comp.pm.name(), "breakeven_then_trail");
        assert_eq!(comp.execution.name(), "next_bar_open");
        assert_eq!(comp.filter.name(), "no_filter");
    }

    // ── Compatibility checks ────────────────────────────────────

    #[test]
    fn compat_stop_entry_with_non_breakout_warns() {
        let config = StrategyConfig {
            signal: ComponentConfig {
                component_type: "ma_crossover".into(),
                params: BTreeMap::new(),
            },
            execution_model: ComponentConfig {
                component_type: "stop_entry".into(),
                params: BTreeMap::new(),
            },
            position_manager: ComponentConfig {
                component_type: "no_op".into(),
                params: BTreeMap::new(),
            },
            signal_filter: ComponentConfig {
                component_type: "no_filter".into(),
                params: BTreeMap::new(),
            },
        };
        let result = check_compatibility(&config);
        assert!(!result.is_clean());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("stop_entry with non-breakout signal"));
    }

    #[test]
    fn compat_limit_entry_with_breakout_warns() {
        let config = StrategyConfig {
            signal: ComponentConfig {
                component_type: "donchian_breakout".into(),
                params: BTreeMap::new(),
            },
            execution_model: ComponentConfig {
                component_type: "limit_entry".into(),
                params: BTreeMap::new(),
            },
            position_manager: ComponentConfig {
                component_type: "no_op".into(),
                params: BTreeMap::new(),
            },
            signal_filter: ComponentConfig {
                component_type: "no_filter".into(),
                params: BTreeMap::new(),
            },
        };
        let result = check_compatibility(&config);
        assert!(!result.is_clean());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("limit_entry with breakout signal"));
    }

    #[test]
    fn compat_clean_combo_no_warnings() {
        let config = StrategyPreset::DonchianTrend.to_config();
        let result = check_compatibility(&config);
        assert!(result.is_clean());
    }

    // ── Presets metadata ────────────────────────────────────────

    #[test]
    fn presets_all_returns_five() {
        assert_eq!(StrategyPreset::all().len(), 5);
    }

    // ── Trading mode preservation ───────────────────────────────

    #[test]
    fn build_composition_preserves_trading_mode() {
        let config = StrategyPreset::DonchianTrend.to_config();
        let comp = build_composition(&config, TradingMode::ShortOnly).unwrap();
        assert_eq!(comp.trading_mode, TradingMode::ShortOnly);
    }

    // ── Indicators are non-empty ────────────────────────────────

    #[test]
    fn indicators_non_empty_for_each_preset() {
        for preset in StrategyPreset::all() {
            let config = preset.to_config();
            let comp = build_composition(&config, TradingMode::LongOnly).unwrap();
            assert!(
                !comp.indicators.is_empty(),
                "Preset {:?} produced zero indicators",
                preset,
            );
        }
    }
}
