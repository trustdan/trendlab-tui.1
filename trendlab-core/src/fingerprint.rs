//! Run fingerprinting — deterministic identification of strategy configurations.
//!
//! - `StrategyConfig`: the four components + their parameters.
//! - `ConfigHash`: structural identity (component types only, no parameter values).
//! - `FullHash`: exact identity (component types + all parameter values).
//! - `RunFingerprint`: complete record of a backtest run for the JSONL history.

use crate::domain::{ConfigHash, DatasetHash, FullHash, RunId};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Configuration of a single component (signal, PM, execution model, or filter).
///
/// Uses `BTreeMap` for deterministic key ordering during serialization → hashing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentConfig {
    pub component_type: String,
    pub params: BTreeMap<String, f64>,
}

/// Complete strategy configuration: four components.
///
/// Produces two hashes:
/// - `config_hash()`: structural only (component types, no param values) — for grouping.
/// - `full_hash()`: structural + params — for exact deduplication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyConfig {
    pub signal: ComponentConfig,
    pub position_manager: ComponentConfig,
    pub execution_model: ComponentConfig,
    pub signal_filter: ComponentConfig,
}

impl StrategyConfig {
    /// Structural hash: only component type names, ignoring parameter values.
    ///
    /// Two Donchian breakout strategies with different lookback periods produce
    /// the same `config_hash` but different `full_hash` values.
    pub fn config_hash(&self) -> ConfigHash {
        let structural = format!(
            "{}+{}+{}+{}",
            self.signal.component_type,
            self.position_manager.component_type,
            self.execution_model.component_type,
            self.signal_filter.component_type,
        );
        ConfigHash::from_bytes(structural.as_bytes())
    }

    /// Full hash: component types + all parameter values.
    ///
    /// Canonical serialization: keys are sorted (BTreeMap) and the JSON is deterministic.
    pub fn full_hash(&self) -> FullHash {
        // serde_json with BTreeMap produces deterministic key order
        let json = serde_json::to_string(self).expect("StrategyConfig must serialize");
        FullHash::from_bytes(json.as_bytes())
    }
}

/// Trading mode: which directions are allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingMode {
    LongOnly,
    ShortOnly,
    LongShort,
}

/// Complete fingerprint of a single backtest run.
///
/// Persisted to JSONL for the YOLO history system. Contains everything needed
/// to reproduce the run or analyze it in the meta-analysis system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunFingerprint {
    // ── Identity ──
    pub run_id: RunId,
    pub timestamp: chrono::NaiveDateTime,
    pub seed: u64,

    // ── Configuration ──
    pub symbol: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub trading_mode: TradingMode,
    pub initial_capital: f64,

    // ── Components ──
    pub strategy_config: StrategyConfig,

    // ── Derived hashes ──
    pub config_hash: ConfigHash,
    pub full_hash: FullHash,
    pub dataset_hash: DatasetHash,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> StrategyConfig {
        StrategyConfig {
            signal: ComponentConfig {
                component_type: "donchian_breakout".into(),
                params: {
                    let mut m = BTreeMap::new();
                    m.insert("entry_lookback".into(), 50.0);
                    m.insert("exit_lookback".into(), 20.0);
                    m
                },
            },
            position_manager: ComponentConfig {
                component_type: "atr_trailing".into(),
                params: {
                    let mut m = BTreeMap::new();
                    m.insert("atr_period".into(), 14.0);
                    m.insert("multiplier".into(), 3.0);
                    m
                },
            },
            execution_model: ComponentConfig {
                component_type: "next_bar_open".into(),
                params: BTreeMap::new(),
            },
            signal_filter: ComponentConfig {
                component_type: "no_filter".into(),
                params: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn config_hash_is_structural() {
        let c1 = sample_config();
        let mut c2 = sample_config();
        // Same structure, different parameters
        c2.signal.params.insert("entry_lookback".into(), 100.0);

        assert_eq!(c1.config_hash(), c2.config_hash());
        assert_ne!(c1.full_hash(), c2.full_hash());
    }

    #[test]
    fn full_hash_differs_for_different_params() {
        let c1 = sample_config();
        let mut c2 = sample_config();
        c2.position_manager.params.insert("multiplier".into(), 5.0);

        assert_ne!(c1.full_hash(), c2.full_hash());
    }

    #[test]
    fn config_hash_differs_for_different_structure() {
        let c1 = sample_config();
        let mut c2 = sample_config();
        c2.signal.component_type = "bollinger_breakout".into();

        assert_ne!(c1.config_hash(), c2.config_hash());
    }

    #[test]
    fn hashing_is_deterministic() {
        let config = sample_config();
        let h1 = config.full_hash();
        let h2 = config.full_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn strategy_config_serialization_roundtrip() {
        let config = sample_config();
        let json = serde_json::to_string(&config).unwrap();
        let deser: StrategyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deser);
        // Hashes must match after roundtrip
        assert_eq!(config.full_hash(), deser.full_hash());
    }

    #[test]
    fn trading_mode_serialization() {
        let mode = TradingMode::LongOnly;
        let json = serde_json::to_string(&mode).unwrap();
        let deser: TradingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deser);
    }
}
