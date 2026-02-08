//! Factory system — converts `ComponentConfig` into runtime trait objects.
//!
//! Four factory functions (`create_signal`, `create_pm`, `create_execution`,
//! `create_filter`) plus a `required_indicators` resolver that inspects configs
//! to determine which indicators need precomputing.

use std::collections::HashSet;

use crate::fingerprint::ComponentConfig;
use crate::indicators::{
    Adx, Aroon, Atr, Bollinger, Donchian, Ema, Keltner, Momentum, ParabolicSar, Roc, Sma,
    Supertrend,
};

use super::execution::{
    CloseOnSignalModel, ExecutionModel, ExecutionPreset, LimitEntryModel, NextBarOpenModel,
    StopEntryModel,
};
use super::filter::{
    AdxFilter, MaRegimeFilter, NoFilter, RegimeDirection, SignalFilter, VolatilityFilter,
};
use super::indicator::Indicator;
use super::pm::{
    AtrTrailing, BreakevenThenTrail, Chandelier, FixedStopLoss, FrozenReference, MaxHoldingPeriod,
    NoOpPm, PercentTrailing, PositionManager, SinceEntryTrailing, TimeDecay,
};
use super::signal::{
    AroonCrossover, BollingerBreakout, Breakout52w, DonchianBreakout, KeltnerBreakout, MaCrossover,
    MaType, ParabolicSarSignal, RocMomentum, SignalGenerator, SupertrendSignal, Tsmom,
};

// ─── Error type ──────────────────────────────────────────────────────

/// Errors that can occur during component construction.
#[derive(Debug, thiserror::Error)]
pub enum FactoryError {
    #[error("Unknown signal type: {0}")]
    UnknownSignal(String),
    #[error("Unknown position manager type: {0}")]
    UnknownPm(String),
    #[error("Unknown execution model type: {0}")]
    UnknownExecution(String),
    #[error("Unknown filter type: {0}")]
    UnknownFilter(String),
}

// ─── Helpers ─────────────────────────────────────────────────────────

/// Extract a named f64 parameter from a `ComponentConfig`, falling back to `default`.
fn param(config: &ComponentConfig, name: &str, default: f64) -> f64 {
    config.params.get(name).copied().unwrap_or(default)
}

/// Extract a named usize parameter from a `ComponentConfig`, falling back to `default`.
fn param_usize(config: &ComponentConfig, name: &str, default: usize) -> usize {
    config
        .params
        .get(name)
        .copied()
        .map(|v| v as usize)
        .unwrap_or(default)
}

// ─── Signal factory ──────────────────────────────────────────────────

/// Create a signal generator from a `ComponentConfig`.
pub fn create_signal(config: &ComponentConfig) -> Result<Box<dyn SignalGenerator>, FactoryError> {
    match config.component_type.as_str() {
        "breakout_52w" => {
            let lookback = param_usize(config, "lookback", 252);
            let threshold_pct = param(config, "threshold_pct", 0.0);
            Ok(Box::new(Breakout52w::new(lookback, threshold_pct)))
        }
        "donchian_breakout" => {
            let entry_lookback = param_usize(config, "entry_lookback", 50);
            Ok(Box::new(DonchianBreakout::new(entry_lookback)))
        }
        "bollinger_breakout" => {
            let period = param_usize(config, "period", 20);
            let std_multiplier = param(config, "std_multiplier", 2.0);
            Ok(Box::new(BollingerBreakout::new(period, std_multiplier)))
        }
        "keltner_breakout" => {
            let ema_period = param_usize(config, "ema_period", 20);
            let atr_period = param_usize(config, "atr_period", 10);
            let multiplier = param(config, "multiplier", 1.5);
            Ok(Box::new(KeltnerBreakout::new(
                ema_period, atr_period, multiplier,
            )))
        }
        "supertrend" => {
            let period = param_usize(config, "period", 10);
            let multiplier = param(config, "multiplier", 3.0);
            Ok(Box::new(SupertrendSignal::new(period, multiplier)))
        }
        "parabolic_sar" => {
            let af_start = param(config, "af_start", 0.02);
            let af_step = param(config, "af_step", 0.02);
            let af_max = param(config, "af_max", 0.20);
            Ok(Box::new(ParabolicSarSignal::new(af_start, af_step, af_max)))
        }
        "ma_crossover" => {
            let fast_period = param_usize(config, "fast_period", 10);
            let slow_period = param_usize(config, "slow_period", 50);
            let ma_type_val = param(config, "ma_type", 0.0);
            let ma_type = if ma_type_val == 1.0 {
                MaType::Ema
            } else {
                MaType::Sma
            };
            Ok(Box::new(MaCrossover::new(
                fast_period,
                slow_period,
                ma_type,
            )))
        }
        "tsmom" => {
            let lookback = param_usize(config, "lookback", 20);
            Ok(Box::new(Tsmom::new(lookback)))
        }
        "roc_momentum" => {
            let period = param_usize(config, "period", 12);
            let threshold_pct = param(config, "threshold_pct", 0.0);
            Ok(Box::new(RocMomentum::new(period, threshold_pct)))
        }
        "aroon_crossover" => {
            let period = param_usize(config, "period", 25);
            Ok(Box::new(AroonCrossover::new(period)))
        }
        other => Err(FactoryError::UnknownSignal(other.to_string())),
    }
}

// ─── PM factory ──────────────────────────────────────────────────────

/// Create a position manager from a `ComponentConfig`.
pub fn create_pm(config: &ComponentConfig) -> Result<Box<dyn PositionManager>, FactoryError> {
    match config.component_type.as_str() {
        "atr_trailing" => {
            let atr_period = param_usize(config, "atr_period", 14);
            let multiplier = param(config, "multiplier", 3.0);
            Ok(Box::new(AtrTrailing::new(atr_period, multiplier)))
        }
        "percent_trailing" => {
            let trail_pct = param(config, "trail_pct", 0.05);
            Ok(Box::new(PercentTrailing::new(trail_pct)))
        }
        "chandelier" => {
            let atr_period = param_usize(config, "atr_period", 22);
            let multiplier = param(config, "multiplier", 3.0);
            Ok(Box::new(Chandelier::new(atr_period, multiplier)))
        }
        "fixed_stop_loss" => {
            let stop_pct = param(config, "stop_pct", 0.02);
            Ok(Box::new(FixedStopLoss::new(stop_pct)))
        }
        "breakeven_then_trail" => {
            let breakeven_trigger_pct = param(config, "breakeven_trigger_pct", 0.02);
            let trail_pct = param(config, "trail_pct", 0.03);
            Ok(Box::new(BreakevenThenTrail::new(
                breakeven_trigger_pct,
                trail_pct,
            )))
        }
        "time_decay" => {
            let initial_pct = param(config, "initial_pct", 0.10);
            let decay_per_bar = param(config, "decay_per_bar", 0.005);
            let min_pct = param(config, "min_pct", 0.02);
            Ok(Box::new(TimeDecay::new(
                initial_pct,
                decay_per_bar,
                min_pct,
            )))
        }
        "frozen_reference" => {
            let exit_pct = param(config, "exit_pct", 0.05);
            Ok(Box::new(FrozenReference::new(exit_pct)))
        }
        "since_entry_trailing" => {
            let exit_pct = param(config, "exit_pct", 0.05);
            Ok(Box::new(SinceEntryTrailing::new(exit_pct)))
        }
        "max_holding_period" => {
            let max_bars = param_usize(config, "max_bars", 20);
            Ok(Box::new(MaxHoldingPeriod::new(max_bars)))
        }
        "no_op" => Ok(Box::new(NoOpPm)),
        other => Err(FactoryError::UnknownPm(other.to_string())),
    }
}

// ─── Execution factory ──────────────────────────────────────────────

/// Map a numeric preset parameter to an `ExecutionPreset`.
fn decode_preset(config: &ComponentConfig) -> ExecutionPreset {
    match param(config, "preset", 1.0) as u8 {
        0 => ExecutionPreset::Frictionless,
        2 => ExecutionPreset::Hostile,
        3 => ExecutionPreset::Optimistic,
        _ => ExecutionPreset::Realistic, // 1.0 = default
    }
}

/// Create an execution model from a `ComponentConfig`.
pub fn create_execution(config: &ComponentConfig) -> Result<Box<dyn ExecutionModel>, FactoryError> {
    let preset = decode_preset(config);
    match config.component_type.as_str() {
        "next_bar_open" => Ok(Box::new(NextBarOpenModel::new(preset))),
        "stop_entry" => Ok(Box::new(StopEntryModel::new(preset))),
        "close_on_signal" => Ok(Box::new(CloseOnSignalModel::new(preset))),
        "limit_entry" => {
            let offset_bps = param(config, "offset_bps", 25.0);
            Ok(Box::new(LimitEntryModel::new(preset, offset_bps)))
        }
        other => Err(FactoryError::UnknownExecution(other.to_string())),
    }
}

// ─── Filter factory ─────────────────────────────────────────────────

/// Create a signal filter from a `ComponentConfig`.
pub fn create_filter(config: &ComponentConfig) -> Result<Box<dyn SignalFilter>, FactoryError> {
    match config.component_type.as_str() {
        "no_filter" => Ok(Box::new(NoFilter)),
        "adx_filter" => {
            let period = param_usize(config, "period", 14);
            let threshold = param(config, "threshold", 25.0);
            Ok(Box::new(AdxFilter::new(period, threshold)))
        }
        "ma_regime" => {
            let period = param_usize(config, "period", 200);
            let direction_val = param(config, "direction", 0.0);
            let regime = if direction_val == 1.0 {
                RegimeDirection::Below
            } else {
                RegimeDirection::Above
            };
            Ok(Box::new(MaRegimeFilter::new(period, regime)))
        }
        "volatility_filter" => {
            let period = param_usize(config, "period", 14);
            let min_pct = param(config, "min_pct", 0.5);
            let max_pct = param(config, "max_pct", 5.0);
            Ok(Box::new(VolatilityFilter::new(period, min_pct, max_pct)))
        }
        other => Err(FactoryError::UnknownFilter(other.to_string())),
    }
}

// ─── Required indicators resolver ───────────────────────────────────

/// Determine which indicators are required by a strategy's components.
///
/// Inspects the signal, filter, and PM configs to build a deduplicated list
/// of indicator trait objects for precomputation before the bar loop.
pub fn required_indicators(
    signal: &ComponentConfig,
    filter: &ComponentConfig,
    pm: &ComponentConfig,
) -> Vec<Box<dyn Indicator>> {
    let mut seen = HashSet::new();
    let mut indicators: Vec<Box<dyn Indicator>> = Vec::new();

    // Helper closure: add an indicator if its name hasn't been seen yet.
    let mut add = |ind: Box<dyn Indicator>| {
        let key = ind.name().to_string();
        if seen.insert(key) {
            indicators.push(ind);
        }
    };

    // ── Signal indicators ────────────────────────────────────────
    match signal.component_type.as_str() {
        "breakout_52w" => {
            let lookback = param_usize(signal, "lookback", 252);
            add(Box::new(Donchian::upper(lookback)));
        }
        "donchian_breakout" => {
            let entry_lookback = param_usize(signal, "entry_lookback", 50);
            add(Box::new(Donchian::upper(entry_lookback)));
        }
        "bollinger_breakout" => {
            let period = param_usize(signal, "period", 20);
            let multiplier = param(signal, "std_multiplier", 2.0);
            add(Box::new(Bollinger::upper(period, multiplier)));
        }
        "keltner_breakout" => {
            let ema_period = param_usize(signal, "ema_period", 20);
            let atr_period = param_usize(signal, "atr_period", 10);
            let multiplier = param(signal, "multiplier", 1.5);
            add(Box::new(Keltner::upper(ema_period, atr_period, multiplier)));
        }
        "supertrend" => {
            let period = param_usize(signal, "period", 10);
            let multiplier = param(signal, "multiplier", 3.0);
            add(Box::new(Supertrend::new(period, multiplier)));
        }
        "parabolic_sar" => {
            let af_start = param(signal, "af_start", 0.02);
            let af_step = param(signal, "af_step", 0.02);
            let af_max = param(signal, "af_max", 0.20);
            add(Box::new(ParabolicSar::new(af_start, af_step, af_max)));
        }
        "ma_crossover" => {
            let fast_period = param_usize(signal, "fast_period", 10);
            let slow_period = param_usize(signal, "slow_period", 50);
            let ma_type_val = param(signal, "ma_type", 0.0);
            if ma_type_val == 1.0 {
                add(Box::new(Ema::new(fast_period)));
                add(Box::new(Ema::new(slow_period)));
            } else {
                add(Box::new(Sma::new(fast_period)));
                add(Box::new(Sma::new(slow_period)));
            }
        }
        "tsmom" => {
            let lookback = param_usize(signal, "lookback", 20);
            add(Box::new(Momentum::new(lookback)));
        }
        "roc_momentum" => {
            let period = param_usize(signal, "period", 12);
            add(Box::new(Roc::new(period)));
        }
        "aroon_crossover" => {
            let period = param_usize(signal, "period", 25);
            add(Box::new(Aroon::up(period)));
            add(Box::new(Aroon::down(period)));
        }
        _ => {} // Unknown signal — no indicators to add.
    }

    // ── Filter indicators ────────────────────────────────────────
    match filter.component_type.as_str() {
        "adx_filter" => {
            let period = param_usize(filter, "period", 14);
            add(Box::new(Adx::new(period)));
        }
        "ma_regime" => {
            let period = param_usize(filter, "period", 200);
            add(Box::new(Sma::new(period)));
        }
        "volatility_filter" => {
            let period = param_usize(filter, "period", 14);
            add(Box::new(Atr::new(period)));
        }
        _ => {} // no_filter or unknown — nothing needed.
    }

    // ── PM indicators (ATR-dependent PMs) ────────────────────────
    match pm.component_type.as_str() {
        "atr_trailing" => {
            let atr_period = param_usize(pm, "atr_period", 14);
            add(Box::new(Atr::new(atr_period)));
        }
        "chandelier" => {
            let atr_period = param_usize(pm, "atr_period", 22);
            add(Box::new(Atr::new(atr_period)));
        }
        _ => {} // Other PMs don't need precomputed indicators.
    }

    indicators
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Helper: build a ComponentConfig with given type and params.
    fn config(component_type: &str, params: &[(&str, f64)]) -> ComponentConfig {
        let mut p = BTreeMap::new();
        for &(k, v) in params {
            p.insert(k.to_string(), v);
        }
        ComponentConfig {
            component_type: component_type.to_string(),
            params: p,
        }
    }

    /// Empty-params config.
    fn bare(component_type: &str) -> ComponentConfig {
        config(component_type, &[])
    }

    // ── param / param_usize helpers ──────────────────────────────

    #[test]
    fn param_returns_value_if_present() {
        let c = config("x", &[("lookback", 42.0)]);
        assert_eq!(param(&c, "lookback", 10.0), 42.0);
    }

    #[test]
    fn param_returns_default_if_missing() {
        let c = bare("x");
        assert_eq!(param(&c, "lookback", 10.0), 10.0);
    }

    #[test]
    fn param_usize_returns_value_if_present() {
        let c = config("x", &[("period", 30.0)]);
        assert_eq!(param_usize(&c, "period", 14), 30);
    }

    #[test]
    fn param_usize_returns_default_if_missing() {
        let c = bare("x");
        assert_eq!(param_usize(&c, "period", 14), 14);
    }

    // ── Signal factory (exhaustive) ──────────────────────────────

    #[test]
    fn signal_breakout_52w() {
        let sig = create_signal(&bare("breakout_52w")).unwrap();
        assert_eq!(sig.name(), "breakout_52w");
    }

    #[test]
    fn signal_donchian_breakout() {
        let sig = create_signal(&bare("donchian_breakout")).unwrap();
        assert_eq!(sig.name(), "donchian_breakout");
    }

    #[test]
    fn signal_bollinger_breakout() {
        let sig = create_signal(&bare("bollinger_breakout")).unwrap();
        assert_eq!(sig.name(), "bollinger_breakout");
    }

    #[test]
    fn signal_keltner_breakout() {
        let sig = create_signal(&bare("keltner_breakout")).unwrap();
        assert_eq!(sig.name(), "keltner_breakout");
    }

    #[test]
    fn signal_supertrend() {
        let sig = create_signal(&bare("supertrend")).unwrap();
        assert_eq!(sig.name(), "supertrend_flip");
    }

    #[test]
    fn signal_parabolic_sar() {
        let sig = create_signal(&bare("parabolic_sar")).unwrap();
        assert_eq!(sig.name(), "parabolic_sar");
    }

    #[test]
    fn signal_ma_crossover_sma_default() {
        let sig = create_signal(&bare("ma_crossover")).unwrap();
        assert_eq!(sig.name(), "ma_crossover");
    }

    #[test]
    fn signal_ma_crossover_ema() {
        let sig = create_signal(&config("ma_crossover", &[("ma_type", 1.0)])).unwrap();
        assert_eq!(sig.name(), "ma_crossover");
    }

    #[test]
    fn signal_tsmom() {
        let sig = create_signal(&bare("tsmom")).unwrap();
        assert_eq!(sig.name(), "tsmom");
    }

    #[test]
    fn signal_roc_momentum() {
        let sig = create_signal(&bare("roc_momentum")).unwrap();
        assert_eq!(sig.name(), "roc_momentum");
    }

    #[test]
    fn signal_aroon_crossover() {
        let sig = create_signal(&bare("aroon_crossover")).unwrap();
        assert_eq!(sig.name(), "aroon_crossover");
    }

    #[test]
    fn signal_unknown_returns_error() {
        let result = create_signal(&bare("bogus_signal"));
        assert!(result.is_err());
        match result.err().unwrap() {
            FactoryError::UnknownSignal(name) => assert_eq!(name, "bogus_signal"),
            other => panic!("expected UnknownSignal, got {:?}", other),
        }
    }

    // ── PM factory (exhaustive) ──────────────────────────────────

    #[test]
    fn pm_atr_trailing() {
        let pm = create_pm(&bare("atr_trailing")).unwrap();
        assert_eq!(pm.name(), "atr_trailing");
    }

    #[test]
    fn pm_percent_trailing() {
        let pm = create_pm(&bare("percent_trailing")).unwrap();
        assert_eq!(pm.name(), "percent_trailing");
    }

    #[test]
    fn pm_chandelier() {
        let pm = create_pm(&bare("chandelier")).unwrap();
        assert_eq!(pm.name(), "chandelier_exit");
    }

    #[test]
    fn pm_fixed_stop_loss() {
        let pm = create_pm(&bare("fixed_stop_loss")).unwrap();
        assert_eq!(pm.name(), "fixed_stop_loss");
    }

    #[test]
    fn pm_breakeven_then_trail() {
        let pm = create_pm(&bare("breakeven_then_trail")).unwrap();
        assert_eq!(pm.name(), "breakeven_then_trail");
    }

    #[test]
    fn pm_time_decay() {
        let pm = create_pm(&bare("time_decay")).unwrap();
        assert_eq!(pm.name(), "time_decay");
    }

    #[test]
    fn pm_frozen_reference() {
        let pm = create_pm(&bare("frozen_reference")).unwrap();
        assert_eq!(pm.name(), "frozen_reference");
    }

    #[test]
    fn pm_since_entry_trailing() {
        let pm = create_pm(&bare("since_entry_trailing")).unwrap();
        assert_eq!(pm.name(), "since_entry_trailing");
    }

    #[test]
    fn pm_max_holding_period() {
        let pm = create_pm(&bare("max_holding_period")).unwrap();
        assert_eq!(pm.name(), "max_holding_period");
    }

    #[test]
    fn pm_no_op() {
        let pm = create_pm(&bare("no_op")).unwrap();
        assert_eq!(pm.name(), "no_op");
    }

    #[test]
    fn pm_unknown_returns_error() {
        let result = create_pm(&bare("bogus_pm"));
        assert!(result.is_err());
        match result.err().unwrap() {
            FactoryError::UnknownPm(name) => assert_eq!(name, "bogus_pm"),
            other => panic!("expected UnknownPm, got {:?}", other),
        }
    }

    // ── Execution factory (exhaustive) ───────────────────────────

    #[test]
    fn exec_next_bar_open() {
        let ex = create_execution(&bare("next_bar_open")).unwrap();
        assert_eq!(ex.name(), "next_bar_open");
    }

    #[test]
    fn exec_stop_entry() {
        let ex = create_execution(&bare("stop_entry")).unwrap();
        assert_eq!(ex.name(), "stop_entry");
    }

    #[test]
    fn exec_close_on_signal() {
        let ex = create_execution(&bare("close_on_signal")).unwrap();
        assert_eq!(ex.name(), "close_on_signal");
    }

    #[test]
    fn exec_limit_entry() {
        let ex = create_execution(&bare("limit_entry")).unwrap();
        assert_eq!(ex.name(), "limit_entry");
    }

    #[test]
    fn exec_unknown_returns_error() {
        let result = create_execution(&bare("bogus_exec"));
        assert!(result.is_err());
        match result.err().unwrap() {
            FactoryError::UnknownExecution(name) => assert_eq!(name, "bogus_exec"),
            other => panic!("expected UnknownExecution, got {:?}", other),
        }
    }

    #[test]
    fn exec_preset_frictionless() {
        let c = config("next_bar_open", &[("preset", 0.0)]);
        let ex = create_execution(&c).unwrap();
        assert_eq!(ex.slippage_bps(), 0.0);
        assert_eq!(ex.commission_bps(), 0.0);
    }

    #[test]
    fn exec_preset_realistic_default() {
        let c = bare("next_bar_open");
        let ex = create_execution(&c).unwrap();
        assert!(ex.slippage_bps() > 0.0);
    }

    #[test]
    fn exec_preset_hostile() {
        let c = config("next_bar_open", &[("preset", 2.0)]);
        let ex = create_execution(&c).unwrap();
        assert!(ex.slippage_bps() > 5.0); // Hostile has 20 bps
    }

    #[test]
    fn exec_preset_optimistic() {
        let c = config("next_bar_open", &[("preset", 3.0)]);
        let ex = create_execution(&c).unwrap();
        assert_eq!(ex.slippage_bps(), 2.0);
    }

    // ── Filter factory (exhaustive) ──────────────────────────────

    #[test]
    fn filter_no_filter() {
        let f = create_filter(&bare("no_filter")).unwrap();
        assert_eq!(f.name(), "no_filter");
    }

    #[test]
    fn filter_adx_filter() {
        let f = create_filter(&bare("adx_filter")).unwrap();
        assert_eq!(f.name(), "adx_filter");
    }

    #[test]
    fn filter_ma_regime_above_default() {
        let f = create_filter(&bare("ma_regime")).unwrap();
        assert_eq!(f.name(), "ma_regime");
    }

    #[test]
    fn filter_ma_regime_below() {
        let f = create_filter(&config("ma_regime", &[("direction", 1.0)])).unwrap();
        assert_eq!(f.name(), "ma_regime");
    }

    #[test]
    fn filter_volatility_filter() {
        let f = create_filter(&bare("volatility_filter")).unwrap();
        assert_eq!(f.name(), "volatility_filter");
    }

    #[test]
    fn filter_unknown_returns_error() {
        let result = create_filter(&bare("bogus_filter"));
        assert!(result.is_err());
        match result.err().unwrap() {
            FactoryError::UnknownFilter(name) => assert_eq!(name, "bogus_filter"),
            other => panic!("expected UnknownFilter, got {:?}", other),
        }
    }

    // ── required_indicators deduplication ────────────────────────

    #[test]
    fn required_indicators_deduplicates_atr() {
        // atr_trailing PM needs atr_14, volatility_filter also needs atr_14.
        let signal = bare("donchian_breakout"); // needs donchian_upper_50
        let filter = config("volatility_filter", &[("period", 14.0)]); // needs atr_14
        let pm = config("atr_trailing", &[("atr_period", 14.0)]); // needs atr_14

        let inds = required_indicators(&signal, &filter, &pm);
        // donchian_upper_50 + atr_14 = 2 (not 3, because atr_14 is deduplicated)
        assert_eq!(inds.len(), 2);

        let names: HashSet<String> = inds.iter().map(|i| i.name().to_string()).collect();
        assert!(names.contains("donchian_upper_50"));
        assert!(names.contains("atr_14"));
    }

    #[test]
    fn required_indicators_correct_count_full_config() {
        // donchian_breakout needs 1 indicator (donchian_upper)
        // adx_filter needs 1 indicator (adx)
        // chandelier needs 1 indicator (atr)
        // All different -> 3 total
        let signal = bare("donchian_breakout");
        let filter = bare("adx_filter");
        let pm = bare("chandelier");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 3);

        let names: HashSet<String> = inds.iter().map(|i| i.name().to_string()).collect();
        assert!(names.contains("donchian_upper_50"));
        assert!(names.contains("adx_14"));
        assert!(names.contains("atr_22"));
    }

    #[test]
    fn required_indicators_ma_crossover_sma() {
        let signal = bare("ma_crossover"); // sma_10 + sma_50
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 2);

        let names: HashSet<String> = inds.iter().map(|i| i.name().to_string()).collect();
        assert!(names.contains("sma_10"));
        assert!(names.contains("sma_50"));
    }

    #[test]
    fn required_indicators_ma_crossover_ema() {
        let signal = config("ma_crossover", &[("ma_type", 1.0)]); // ema_10 + ema_50
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 2);

        let names: HashSet<String> = inds.iter().map(|i| i.name().to_string()).collect();
        assert!(names.contains("ema_10"));
        assert!(names.contains("ema_50"));
    }

    #[test]
    fn required_indicators_aroon_crossover() {
        let signal = bare("aroon_crossover"); // aroon_up_25 + aroon_down_25
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 2);

        let names: HashSet<String> = inds.iter().map(|i| i.name().to_string()).collect();
        assert!(names.contains("aroon_up_25"));
        assert!(names.contains("aroon_down_25"));
    }

    #[test]
    fn required_indicators_no_filter_no_pm_indicators() {
        let signal = bare("tsmom"); // momentum_20
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].name(), "momentum_20");
    }

    #[test]
    fn required_indicators_ma_regime_filter_adds_sma() {
        let signal = bare("roc_momentum"); // roc_12
        let filter = bare("ma_regime"); // sma_200
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 2);

        let names: HashSet<String> = inds.iter().map(|i| i.name().to_string()).collect();
        assert!(names.contains("roc_12"));
        assert!(names.contains("sma_200"));
    }

    #[test]
    fn required_indicators_supertrend_signal() {
        let signal = bare("supertrend"); // supertrend_10_3
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].name(), "supertrend_10_3");
    }

    #[test]
    fn required_indicators_parabolic_sar_signal() {
        let signal = bare("parabolic_sar"); // psar_0.02_0.02_0.2
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].name(), "psar_0.02_0.02_0.2");
    }

    #[test]
    fn required_indicators_bollinger_signal() {
        let signal = bare("bollinger_breakout"); // bollinger_upper_20_2
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].name(), "bollinger_upper_20_2");
    }

    #[test]
    fn required_indicators_keltner_signal() {
        let signal = bare("keltner_breakout"); // keltner_upper_20_10_1.5
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].name(), "keltner_upper_20_10_1.5");
    }

    #[test]
    fn required_indicators_breakout_52w_signal() {
        let signal = bare("breakout_52w"); // donchian_upper_252
        let filter = bare("no_filter");
        let pm = bare("no_op");

        let inds = required_indicators(&signal, &filter, &pm);
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].name(), "donchian_upper_252");
    }

    #[test]
    fn signal_with_custom_params() {
        let sig =
            create_signal(&config("donchian_breakout", &[("entry_lookback", 100.0)])).unwrap();
        assert_eq!(sig.warmup_bars(), 100);
    }

    #[test]
    fn pm_with_custom_params() {
        let pm = create_pm(&config("max_holding_period", &[("max_bars", 50.0)])).unwrap();
        assert_eq!(pm.name(), "max_holding_period");
    }
}
