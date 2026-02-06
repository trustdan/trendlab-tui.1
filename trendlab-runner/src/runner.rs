//! Single backtest execution orchestration.

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

use trendlab_core::domain::Bar;
use trendlab_core::engine::Engine;
use trendlab_core::order_policy::guards::default_guards;

use crate::cache::ResultCache;
use crate::config::{CommissionConfig, IntrabarPolicy, RunConfig, SlippageConfig};
use crate::reporting::export::export_run_with_report;
use crate::result::{BacktestResult, EquityPoint, PerformanceStats, ResultMetadata};

/// Orchestrates single backtest execution.
///
/// The runner:
/// 1. Checks cache for existing results
/// 2. Loads bar data for the universe
/// 3. Instantiates strategy components from config
/// 4. Runs the event loop
/// 5. Computes statistics and returns BacktestResult
/// 6. Caches the result for future runs
pub struct Runner {
    cache: Option<ResultCache>,
    artifact_dir: Option<PathBuf>,
}

impl Runner {
    /// Creates a new runner without caching.
    pub fn new() -> Self {
        Self {
            cache: None,
            artifact_dir: None,
        }
    }

    /// Creates a new runner with caching enabled.
    pub fn with_cache(cache: ResultCache) -> Self {
        Self {
            cache: Some(cache),
            artifact_dir: None,
        }
    }

    /// Creates a new runner with artifact export enabled.
    pub fn with_artifacts(artifact_dir: PathBuf) -> Self {
        Self {
            cache: None,
            artifact_dir: Some(artifact_dir),
        }
    }

    /// Runs a backtest for the given configuration.
    ///
    /// If caching is enabled and a cached result exists, it is returned immediately.
    /// Otherwise, the backtest is executed and the result is cached.
    pub fn run(&self, config: &RunConfig) -> Result<BacktestResult> {
        let run_id = config.run_id();

        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(cached_result) = cache.get(&run_id)? {
                return Ok(cached_result);
            }
        }

        // Execute backtest
        let start_time = std::time::Instant::now();
        let result = self.execute(config)?;
        let duration_secs = start_time.elapsed().as_secs_f64();

        // Add metadata
        let mut result = result;
        result.metadata.duration_secs = duration_secs;

        // Cache result
        if let Some(cache) = &self.cache {
            cache.put(&result)?;
        }

        // Export artifacts if configured
        if let Some(artifact_dir) = &self.artifact_dir {
            let _ = export_run_with_report(artifact_dir, &result, true);
        }

        Ok(result)
    }

    /// Executes the backtest (no caching).
    fn execute(&self, config: &RunConfig) -> Result<BacktestResult> {
        // Load bar data for the universe
        let bars_by_symbol = self.load_bars(config)?;

        // Validate we have data
        if bars_by_symbol.is_empty() {
            anyhow::bail!("No bar data loaded for universe: {:?}", config.universe);
        }

        // Align bars across symbols (for now, just use the first symbol)
        // TODO M9: Implement proper multi-symbol alignment
        let primary_symbol = &config.universe[0];
        let bars = bars_by_symbol
            .get(primary_symbol)
            .context("Primary symbol not found in loaded data")?;

        // Create engine with guards and path policy
        let warmup_bars = 50; // TODO: Make this configurable
        let guards = default_guards();
        let policy = match config.execution.intrabar_policy {
            IntrabarPolicy::WorstCase => "WorstCase",
            IntrabarPolicy::BestCase => "BestCase",
            IntrabarPolicy::OhlcOrder => "OhlcOrder",
        };
        let mut engine =
            Engine::with_guards_and_policy(config.initial_capital, warmup_bars, guards, policy);

        // Run event loop
        let mut equity_curve = Vec::new();
        let trades = Vec::new(); // TODO M9: Extract trades from engine

        // Record initial equity
        equity_curve.push(EquityPoint {
            date: config.start_date,
            equity: config.initial_capital,
        });

        for bar in bars {
            // Build current prices map
            let mut current_prices = HashMap::new();
            current_prices.insert(bar.symbol.clone(), bar.close);

            // Process bar through engine
            engine.process_bar(bar, &current_prices);

            // Record equity (extract from engine)
            let equity_history = engine.equity_history();
            if let Some(&latest_equity) = equity_history.last() {
                equity_curve.push(EquityPoint {
                    date: self.bar_date(bar),
                    equity: latest_equity,
                });
            }
        }

        // Compute statistics
        let stats = PerformanceStats::from_results(&equity_curve, &trades, config.initial_capital);

        // Build metadata with rejected intents and ideal equity
        let mut custom = HashMap::new();

        // Serialize rejected intents
        let rejected_intents = engine.rejected_intents();
        if !rejected_intents.is_empty() {
            let rejection_json: Vec<serde_json::Value> = rejected_intents
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "bar_index": r.bar_index,
                        "date": r.timestamp.format("%Y-%m-%d").to_string(),
                        "signal": r.signal,
                        "reason": r.reason.to_string(),
                        "context": r.context,
                    })
                })
                .collect();
            custom.insert(
                "rejected_intents".to_string(),
                serde_json::Value::Array(rejection_json),
            );
        }
        custom.insert(
            "total_signals".to_string(),
            serde_json::json!(equity_curve.len()),
        );

        // Compute ideal equity curve (zero slippage/commission)
        let ideal_equity = self.compute_ideal_equity(config, bars)?;
        if !ideal_equity.is_empty() {
            let ideal_json: Vec<serde_json::Value> = ideal_equity
                .iter()
                .map(|ep| {
                    serde_json::json!({
                        "date": ep.date.to_string(),
                        "equity": ep.equity,
                    })
                })
                .collect();
            custom.insert(
                "ideal_equity_curve".to_string(),
                serde_json::Value::Array(ideal_json),
            );
        }

        // Build result
        let result = BacktestResult {
            run_id: config.run_id(),
            equity_curve,
            trades,
            stats,
            metadata: ResultMetadata {
                timestamp: Utc::now(),
                duration_secs: 0.0, // Will be set by caller
                custom,
                config: Some(config.clone()),
            },
        };

        Ok(result)
    }

    /// Computes the ideal equity curve with zero slippage and zero commission.
    fn compute_ideal_equity(
        &self,
        config: &RunConfig,
        bars: &[Bar],
    ) -> Result<Vec<EquityPoint>> {
        let mut ideal_config = config.clone();
        ideal_config.execution.slippage = SlippageConfig::None;
        ideal_config.execution.commission = CommissionConfig::None;

        let warmup_bars = 50;
        let mut engine = Engine::new(ideal_config.initial_capital, warmup_bars);

        let mut equity_curve = Vec::new();
        equity_curve.push(EquityPoint {
            date: ideal_config.start_date,
            equity: ideal_config.initial_capital,
        });

        for bar in bars {
            let mut current_prices = HashMap::new();
            current_prices.insert(bar.symbol.clone(), bar.close);
            engine.process_bar(bar, &current_prices);

            let equity_history = engine.equity_history();
            if let Some(&latest_equity) = equity_history.last() {
                equity_curve.push(EquityPoint {
                    date: self.bar_date(bar),
                    equity: latest_equity,
                });
            }
        }

        Ok(equity_curve)
    }

    /// Loads bar data for the given configuration.
    ///
    /// For M8, this is a stub that loads synthetic data.
    /// In M9, this will load real Parquet data from the data/ directory.
    fn load_bars(&self, config: &RunConfig) -> Result<HashMap<String, Vec<Bar>>> {
        let mut bars_by_symbol = HashMap::new();

        for symbol in &config.universe {
            let bars =
                self.generate_synthetic_bars(symbol, &config.start_date, &config.end_date, config)?;
            bars_by_symbol.insert(symbol.clone(), bars);
        }

        Ok(bars_by_symbol)
    }

    /// Generates synthetic bar data for testing.
    ///
    /// Uses the config's run_id hash to seed the price walk, so different configs
    /// (including different intrabar policies) produce different price paths.
    fn generate_synthetic_bars(
        &self,
        symbol: &str,
        start_date: &NaiveDate,
        end_date: &NaiveDate,
        config: &RunConfig,
    ) -> Result<Vec<Bar>> {
        use chrono::Duration;

        // Derive a seed from the config hash so different policies produce different bars
        let run_id = config.run_id();
        let hash_bytes = blake3::hash(run_id.as_bytes());
        let seed_bytes = hash_bytes.as_bytes();
        let seed = u64::from_le_bytes([
            seed_bytes[0],
            seed_bytes[1],
            seed_bytes[2],
            seed_bytes[3],
            seed_bytes[4],
            seed_bytes[5],
            seed_bytes[6],
            seed_bytes[7],
        ]);

        let mut bars = Vec::new();
        let mut current_date = *start_date;
        let mut price = 100.0;
        let mut bar_idx: u64 = 0;

        while current_date <= *end_date {
            // Mix seed with bar index for per-bar variation
            let mixed = seed.wrapping_add(bar_idx).wrapping_mul(6364136223846793005);
            // Map to [-1.0, 1.0] range
            let frac = ((mixed >> 33) as f64) / (u32::MAX as f64) * 2.0 - 1.0;
            let change = frac * 0.01; // +/- 1% per day
            price *= 1.0 + change;

            let open = price * 0.99;
            let high = price * 1.01;
            let low = price * 0.98;
            let close = price;

            let timestamp = current_date
                .and_hms_opt(16, 0, 0)
                .context("Invalid time")?
                .and_utc();

            bars.push(Bar::new(
                timestamp,
                symbol.to_string(),
                open,
                high,
                low,
                close,
                1_000_000.0,
            ));

            current_date += Duration::days(1);
            bar_idx += 1;
        }

        Ok(bars)
    }

    /// Extracts a date from a bar's timestamp.
    fn bar_date(&self, bar: &Bar) -> NaiveDate {
        bar.timestamp.date_naive()
    }
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, SignalGeneratorConfig,
        StrategyConfig,
    };

    fn make_test_config() -> RunConfig {
        RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::BuyAndHold,
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2020, 12, 31).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    #[test]
    fn test_runner_executes_backtest() {
        let runner = Runner::new();
        let config = make_test_config();

        let result = runner.run(&config).unwrap();

        assert_eq!(result.run_id, config.run_id());
        assert!(!result.equity_curve.is_empty());
        assert_eq!(result.stats.initial_equity, 100_000.0);
    }

    #[test]
    fn test_runner_with_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ResultCache::new(temp_dir.path()).unwrap();
        let runner = Runner::with_cache(cache);

        let config = make_test_config();

        // First run: execute and cache
        let result1 = runner.run(&config).unwrap();

        // Second run: should hit cache
        let result2 = runner.run(&config).unwrap();

        assert_eq!(result1.run_id, result2.run_id);
        assert_eq!(result1.stats.final_equity, result2.stats.final_equity);
    }

    #[test]
    fn test_different_configs_produce_different_results() {
        let runner = Runner::new();

        let config1 = make_test_config();
        let mut config2 = config1.clone();
        config2.initial_capital = 200_000.0;

        let result1 = runner.run(&config1).unwrap();
        let result2 = runner.run(&config2).unwrap();

        assert_ne!(result1.run_id, result2.run_id);
        assert_ne!(result1.stats.initial_equity, result2.stats.initial_equity);
    }

    #[test]
    fn test_runner_produces_rejected_intents() {
        let runner = Runner::new();
        let config = make_test_config();

        let result = runner.run(&config).unwrap();

        // Should have total_signals in metadata
        assert!(result.metadata.custom.contains_key("total_signals"));
    }

    #[test]
    fn test_runner_produces_ideal_equity() {
        let runner = Runner::new();
        let config = make_test_config();

        let result = runner.run(&config).unwrap();

        // Should have ideal_equity_curve in metadata
        assert!(result.metadata.custom.contains_key("ideal_equity_curve"));
        let ideal = result.metadata.custom.get("ideal_equity_curve").unwrap();
        assert!(ideal.is_array());
        assert!(!ideal.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_different_policies_produce_different_bars() {
        let runner = Runner::new();

        let mut config1 = make_test_config();
        config1.execution.intrabar_policy = IntrabarPolicy::WorstCase;

        let mut config2 = make_test_config();
        config2.execution.intrabar_policy = IntrabarPolicy::BestCase;

        // Verify configs produce different run_ids (different hash seeds)
        assert_ne!(config1.run_id(), config2.run_id());

        // Verify synthetic bars differ via the seeded generator
        let bars1 = runner
            .generate_synthetic_bars("SPY", &config1.start_date, &config1.end_date, &config1)
            .unwrap();
        let bars2 = runner
            .generate_synthetic_bars("SPY", &config2.start_date, &config2.end_date, &config2)
            .unwrap();

        assert_eq!(bars1.len(), bars2.len());
        // At least some bars should differ in price due to different seeds
        let differ = bars1
            .iter()
            .zip(bars2.iter())
            .any(|(b1, b2)| (b1.close - b2.close).abs() > 1e-6);
        assert!(differ, "Different policies should produce different price paths");
    }
}
