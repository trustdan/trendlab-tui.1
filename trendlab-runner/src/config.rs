//! Serializable backtest configuration.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a backtest run (content-addressable hash).
pub type RunId = String;

/// Serializable configuration for a single backtest run.
///
/// This struct captures all parameters needed to reproduce a backtest:
/// - Strategy components (signal, order policy, sizer)
/// - Date range and universe
/// - Execution model settings
/// - Initial capital
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunConfig {
    /// Strategy configuration
    pub strategy: StrategyConfig,

    /// Backtest start date (inclusive)
    pub start_date: NaiveDate,

    /// Backtest end date (inclusive)
    pub end_date: NaiveDate,

    /// Universe of symbols to trade
    pub universe: Vec<String>,

    /// Execution model settings
    pub execution: ExecutionConfig,

    /// Initial capital
    pub initial_capital: f64,
}

impl RunConfig {
    /// Computes a deterministic hash ID for this configuration.
    ///
    /// This enables cache lookups: two runs with identical configs
    /// will have the same RunId and can share cached results.
    pub fn run_id(&self) -> RunId {
        let json = serde_json::to_string(self).expect("RunConfig serialization failed");
        let hash = blake3::hash(json.as_bytes());
        format!("{}", hash.to_hex())
    }
}

/// Strategy configuration: signal generator, order policy, and sizer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyConfig {
    pub signal_generator: SignalGeneratorConfig,
    pub order_policy: OrderPolicyConfig,
    pub position_sizer: PositionSizerConfig,
}

/// Signal generator configuration (serializable enum).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalGeneratorConfig {
    /// Moving average crossover: short MA crosses long MA.
    MaCrossover { short_period: usize, long_period: usize },

    /// Buy-and-hold: always long from start to end.
    BuyAndHold,

    /// Custom signal generator with arbitrary parameters.
    Custom { name: String, params: HashMap<String, serde_json::Value> },
}

/// Order policy configuration (serializable enum).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderPolicyConfig {
    /// Simple policy: market orders at next open.
    Simple,

    /// Custom order policy with arbitrary parameters.
    Custom { name: String, params: HashMap<String, serde_json::Value> },
}

/// Position sizer configuration (serializable enum).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionSizerConfig {
    /// Fixed dollar amount per position.
    FixedDollar { amount: f64 },

    /// Fixed number of shares per position.
    FixedShares { shares: i64 },

    /// Percentage of equity per position.
    PercentEquity { percent: f64 },

    /// Custom sizer with arbitrary parameters.
    Custom { name: String, params: HashMap<String, serde_json::Value> },
}

/// Execution model configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionConfig {
    /// Slippage model
    pub slippage: SlippageConfig,

    /// Commission per trade
    pub commission: CommissionConfig,

    /// Intrabar ambiguity resolution policy
    pub intrabar_policy: IntrabarPolicy,
}

/// Slippage configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SlippageConfig {
    /// Fixed slippage in basis points (1 bp = 0.01%)
    FixedBps { bps: f64 },

    /// Percentage of price
    Percentage { percent: f64 },

    /// No slippage (ideal case)
    None,
}

/// Commission configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommissionConfig {
    /// Fixed per-trade commission
    PerTrade { amount: f64 },

    /// Per-share commission
    PerShare { amount: f64 },

    /// Percentage of trade value
    Percentage { percent: f64 },

    /// No commission
    None,
}

/// Intrabar ambiguity resolution policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntrabarPolicy {
    /// Assume worst-case ordering (conservative)
    WorstCase,

    /// Assume best-case ordering (optimistic)
    BestCase,

    /// Use OHLC order heuristic (open → high → low → close if close > open)
    OhlcOrder,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            slippage: SlippageConfig::FixedBps { bps: 5.0 },
            commission: CommissionConfig::PerShare { amount: 0.005 },
            intrabar_policy: IntrabarPolicy::WorstCase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_id_deterministic() {
        let config = RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::MaCrossover {
                    short_period: 10,
                    long_period: 50,
                },
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            universe: vec!["SPY".to_string(), "QQQ".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        };

        let id1 = config.run_id();
        let id2 = config.run_id();

        assert_eq!(id1, id2, "RunId should be deterministic");
        assert!(!id1.is_empty());
    }

    #[test]
    fn test_run_id_changes_with_params() {
        let config1 = RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::MaCrossover {
                    short_period: 10,
                    long_period: 50,
                },
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        };

        let mut config2 = config1.clone();
        config2.strategy.signal_generator = SignalGeneratorConfig::MaCrossover {
            short_period: 20,
            long_period: 50,
        };

        assert_ne!(
            config1.run_id(),
            config2.run_id(),
            "Different configs should have different RunIds"
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::MaCrossover {
                    short_period: 10,
                    long_period: 50,
                },
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: RunConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }
}
