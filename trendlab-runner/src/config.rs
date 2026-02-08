//! TOML config parsing — loads strategy configurations from TOML files.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use trendlab_core::fingerprint::{ComponentConfig, StrategyConfig, TradingMode};

/// Top-level backtest configuration from a TOML file.
#[derive(Debug, Deserialize)]
pub struct BacktestConfig {
    pub backtest: BacktestSection,
    pub signal: ComponentSection,
    pub position_manager: ComponentSection,
    pub execution_model: ComponentSection,
    #[serde(default = "default_no_filter")]
    pub signal_filter: ComponentSection,
}

/// General backtest parameters.
#[derive(Debug, Deserialize)]
pub struct BacktestSection {
    pub symbol: String,
    pub start_date: String,
    pub end_date: String,
    #[serde(default = "default_capital")]
    pub initial_capital: f64,
    #[serde(default = "default_trading_mode")]
    pub trading_mode: String,
    #[serde(default = "default_position_size")]
    pub position_size_pct: f64,
}

/// A component (signal, PM, execution, filter) section in TOML.
#[derive(Debug, Deserialize)]
pub struct ComponentSection {
    #[serde(rename = "type")]
    pub component_type: String,
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
}

fn default_capital() -> f64 {
    100_000.0
}
fn default_trading_mode() -> String {
    "long_only".to_string()
}
fn default_position_size() -> f64 {
    1.0
}
fn default_no_filter() -> ComponentSection {
    ComponentSection {
        component_type: "no_filter".to_string(),
        params: BTreeMap::new(),
    }
}

impl BacktestConfig {
    /// Load from a TOML file path.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path).map_err(|e| ConfigError::Io(e.to_string()))?;
        Self::from_toml(&contents)
    }

    /// Parse from a TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, ConfigError> {
        toml::from_str(toml_str).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Convert to a StrategyConfig for the factory system.
    pub fn to_strategy_config(&self) -> StrategyConfig {
        StrategyConfig {
            signal: ComponentConfig {
                component_type: self.signal.component_type.clone(),
                params: self.signal.params.clone(),
            },
            position_manager: ComponentConfig {
                component_type: self.position_manager.component_type.clone(),
                params: self.position_manager.params.clone(),
            },
            execution_model: ComponentConfig {
                component_type: self.execution_model.component_type.clone(),
                params: self.execution_model.params.clone(),
            },
            signal_filter: ComponentConfig {
                component_type: self.signal_filter.component_type.clone(),
                params: self.signal_filter.params.clone(),
            },
        }
    }

    /// Parse the trading mode string.
    pub fn trading_mode(&self) -> TradingMode {
        match self.backtest.trading_mode.as_str() {
            "long_only" => TradingMode::LongOnly,
            "short_only" => TradingMode::ShortOnly,
            "long_short" => TradingMode::LongShort,
            _ => TradingMode::LongOnly,
        }
    }
}

/// Config loading errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("TOML parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_TOML: &str = r#"
[backtest]
symbol = "AAPL"
start_date = "2020-01-01"
end_date = "2023-12-31"
initial_capital = 50000.0
trading_mode = "long_only"
position_size_pct = 0.5

[signal]
type = "donchian_breakout"
params = { entry_lookback = 50.0, exit_lookback = 20.0 }

[position_manager]
type = "atr_trailing"
params = { atr_period = 14.0, multiplier = 3.0 }

[execution_model]
type = "next_bar_open"

[signal_filter]
type = "sma_regime"
params = { period = 200.0 }
"#;

    const MINIMAL_TOML: &str = r#"
[backtest]
symbol = "SPY"
start_date = "2020-01-01"
end_date = "2023-12-31"

[signal]
type = "donchian_breakout"
params = { entry_lookback = 50.0 }

[position_manager]
type = "atr_trailing"
params = { atr_period = 14.0 }

[execution_model]
type = "next_bar_open"
"#;

    #[test]
    fn parse_valid_toml() {
        let config = BacktestConfig::from_toml(FULL_TOML).unwrap();
        assert_eq!(config.backtest.symbol, "AAPL");
        assert_eq!(config.backtest.start_date, "2020-01-01");
        assert_eq!(config.backtest.end_date, "2023-12-31");
        assert_eq!(config.backtest.initial_capital, 50_000.0);
        assert_eq!(config.backtest.trading_mode, "long_only");
        assert_eq!(config.backtest.position_size_pct, 0.5);

        assert_eq!(config.signal.component_type, "donchian_breakout");
        assert_eq!(config.signal.params["entry_lookback"], 50.0);
        assert_eq!(config.signal.params["exit_lookback"], 20.0);

        assert_eq!(config.position_manager.component_type, "atr_trailing");
        assert_eq!(config.position_manager.params["atr_period"], 14.0);
        assert_eq!(config.position_manager.params["multiplier"], 3.0);

        assert_eq!(config.execution_model.component_type, "next_bar_open");
        assert!(config.execution_model.params.is_empty());

        assert_eq!(config.signal_filter.component_type, "sma_regime");
        assert_eq!(config.signal_filter.params["period"], 200.0);
    }

    #[test]
    fn convert_to_strategy_config() {
        let config = BacktestConfig::from_toml(FULL_TOML).unwrap();
        let sc = config.to_strategy_config();

        assert_eq!(sc.signal.component_type, "donchian_breakout");
        assert_eq!(sc.signal.params["entry_lookback"], 50.0);
        assert_eq!(sc.signal.params["exit_lookback"], 20.0);

        assert_eq!(sc.position_manager.component_type, "atr_trailing");
        assert_eq!(sc.position_manager.params["atr_period"], 14.0);
        assert_eq!(sc.position_manager.params["multiplier"], 3.0);

        assert_eq!(sc.execution_model.component_type, "next_bar_open");
        assert!(sc.execution_model.params.is_empty());

        assert_eq!(sc.signal_filter.component_type, "sma_regime");
        assert_eq!(sc.signal_filter.params["period"], 200.0);
    }

    #[test]
    fn default_filter_when_omitted() {
        let config = BacktestConfig::from_toml(MINIMAL_TOML).unwrap();
        assert_eq!(config.signal_filter.component_type, "no_filter");
        assert!(config.signal_filter.params.is_empty());

        // Also verify defaults for other optional fields
        assert_eq!(config.backtest.initial_capital, 100_000.0);
        assert_eq!(config.backtest.trading_mode, "long_only");
        assert_eq!(config.backtest.position_size_pct, 1.0);
    }

    #[test]
    fn trading_mode_parsing() {
        // long_only
        let config = BacktestConfig::from_toml(FULL_TOML).unwrap();
        assert_eq!(config.trading_mode(), TradingMode::LongOnly);

        // short_only
        let toml_short = FULL_TOML.replace("long_only", "short_only");
        let config = BacktestConfig::from_toml(&toml_short).unwrap();
        assert_eq!(config.trading_mode(), TradingMode::ShortOnly);

        // long_short
        let toml_both = FULL_TOML.replace("long_only", "long_short");
        let config = BacktestConfig::from_toml(&toml_both).unwrap();
        assert_eq!(config.trading_mode(), TradingMode::LongShort);

        // unknown defaults to LongOnly
        let toml_unknown = FULL_TOML.replace("long_only", "sideways_only");
        let config = BacktestConfig::from_toml(&toml_unknown).unwrap();
        assert_eq!(config.trading_mode(), TradingMode::LongOnly);
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        let bad_toml = "this is not [valid toml !!!";
        let result = BacktestConfig::from_toml(bad_toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
        // Verify error message contains something useful
        let msg = err.to_string();
        assert!(msg.contains("TOML parse error"));
    }
}
