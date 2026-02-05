//! Single backtest execution orchestration.

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate, Utc};
use std::collections::HashMap;

use trendlab_core::domain::Bar;
use trendlab_core::engine::Engine;

use crate::cache::ResultCache;
use crate::config::RunConfig;
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
}

impl Runner {
    /// Creates a new runner without caching.
    pub fn new() -> Self {
        Self { cache: None }
    }

    /// Creates a new runner with caching enabled.
    pub fn with_cache(cache: ResultCache) -> Self {
        Self { cache: Some(cache) }
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

        // Create engine
        let warmup_bars = 50; // TODO: Make this configurable
        let mut engine = Engine::new(config.initial_capital, warmup_bars);

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

        // Build result
        let result = BacktestResult {
            run_id: config.run_id(),
            equity_curve,
            trades,
            stats,
            metadata: ResultMetadata {
                timestamp: Utc::now(),
                duration_secs: 0.0, // Will be set by caller
                custom: HashMap::new(),
            },
        };

        Ok(result)
    }

    /// Loads bar data for the given configuration.
    ///
    /// For M8, this is a stub that loads synthetic data.
    /// In M9, this will load real Parquet data from the data/ directory.
    fn load_bars(&self, config: &RunConfig) -> Result<HashMap<String, Vec<Bar>>> {
        let mut bars_by_symbol = HashMap::new();

        for symbol in &config.universe {
            let bars = self.generate_synthetic_bars(symbol, &config.start_date, &config.end_date)?;
            bars_by_symbol.insert(symbol.clone(), bars);
        }

        Ok(bars_by_symbol)
    }

    /// Generates synthetic bar data for testing.
    ///
    /// Creates a simple random walk with daily bars.
    fn generate_synthetic_bars(
        &self,
        symbol: &str,
        start_date: &NaiveDate,
        end_date: &NaiveDate,
    ) -> Result<Vec<Bar>> {
        use chrono::Duration;

        let mut bars = Vec::new();
        let mut current_date = *start_date;
        let mut price = 100.0;

        while current_date <= *end_date {
            // Simple random walk: +/- 1% per day
            let change = (current_date.ordinal() % 3) as f64 - 1.0;
            price *= 1.0 + (change * 0.01);

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
}
