//! Parameter sweep utilities for grid/random search.

use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;

use crate::config::{RunConfig, SignalGeneratorConfig};
use crate::result::BacktestResult;
use crate::runner::Runner;

/// Parameter grid specification.
///
/// Defines ranges for each parameter to sweep over.
#[derive(Debug, Clone)]
pub struct ParamGrid {
    /// MA crossover short periods to test
    pub ma_short_periods: Vec<usize>,

    /// MA crossover long periods to test
    pub ma_long_periods: Vec<usize>,

    /// Initial capital values to test
    pub initial_capitals: Vec<f64>,

    /// Universe variations (for future use)
    pub universes: Vec<Vec<String>>,
}

impl ParamGrid {
    /// Creates a simple grid for MA crossover strategy.
    ///
    /// Short periods: 10, 20, 30
    /// Long periods: 50, 100, 200
    pub fn ma_crossover_default() -> Self {
        Self {
            ma_short_periods: vec![10, 20, 30],
            ma_long_periods: vec![50, 100, 200],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        }
    }

    /// Returns the total number of configurations in this grid.
    pub fn size(&self) -> usize {
        self.ma_short_periods.len()
            * self.ma_long_periods.len()
            * self.initial_capitals.len()
            * self.universes.len()
    }

    /// Generates all configurations in the grid.
    pub fn generate_configs(&self, base_config: &RunConfig) -> Vec<RunConfig> {
        let mut configs = Vec::new();

        for &short in &self.ma_short_periods {
            for &long in &self.ma_long_periods {
                // Skip invalid combinations (short >= long)
                if short >= long {
                    continue;
                }

                for &capital in &self.initial_capitals {
                    for universe in &self.universes {
                        let mut config = base_config.clone();
                        config.strategy.signal_generator = SignalGeneratorConfig::MaCrossover {
                            short_period: short,
                            long_period: long,
                        };
                        config.initial_capital = capital;
                        config.universe = universe.clone();

                        configs.push(config);
                    }
                }
            }
        }

        configs
    }
}

/// Parameter sweep executor.
///
/// Runs backtests for all configurations in a grid, optionally in parallel.
pub struct ParamSweep {
    runner: Runner,
    parallel: bool,
}

impl ParamSweep {
    /// Creates a new parameter sweep with the given runner.
    pub fn new(runner: Runner) -> Self {
        Self {
            runner,
            parallel: true,
        }
    }

    /// Enables or disables parallel execution.
    pub fn with_parallelism(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Executes a parameter sweep over the given grid.
    ///
    /// Returns a map of RunId -> BacktestResult for all configurations.
    ///
    /// If caching is enabled in the runner, previously computed results
    /// will be retrieved from the cache instead of recomputed.
    pub fn sweep(&self, grid: &ParamGrid, base_config: &RunConfig) -> Result<SweepResults> {
        let configs = grid.generate_configs(base_config);

        let results: Vec<BacktestResult> = if self.parallel {
            // Parallel execution using Rayon
            configs
                .par_iter()
                .map(|config| self.runner.run(config))
                .collect::<Result<Vec<_>>>()?
        } else {
            // Sequential execution
            configs
                .iter()
                .map(|config| self.runner.run(config))
                .collect::<Result<Vec<_>>>()?
        };

        Ok(SweepResults::new(results))
    }

    /// Executes a sweep with progress reporting.
    ///
    /// The callback is invoked after each backtest completes with:
    /// - Current index (0-based)
    /// - Total number of configs
    /// - The completed result
    pub fn sweep_with_progress<F>(
        &self,
        grid: &ParamGrid,
        base_config: &RunConfig,
        progress_callback: F,
    ) -> Result<SweepResults>
    where
        F: Fn(usize, usize, &BacktestResult) + Send + Sync,
    {
        let configs = grid.generate_configs(base_config);
        let total = configs.len();

        let results: Vec<BacktestResult> = if self.parallel {
            configs
                .par_iter()
                .enumerate()
                .map(|(idx, config)| {
                    let result = self.runner.run(config)?;
                    progress_callback(idx, total, &result);
                    Ok(result)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            configs
                .iter()
                .enumerate()
                .map(|(idx, config)| {
                    let result = self.runner.run(config)?;
                    progress_callback(idx, total, &result);
                    Ok(result)
                })
                .collect::<Result<Vec<_>>>()?
        };

        Ok(SweepResults::new(results))
    }
}

/// Results from a parameter sweep.
#[derive(Debug)]
pub struct SweepResults {
    results: Vec<BacktestResult>,
    by_run_id: HashMap<String, BacktestResult>,
}

impl SweepResults {
    fn new(results: Vec<BacktestResult>) -> Self {
        let by_run_id = results
            .iter()
            .map(|r| (r.run_id.clone(), r.clone()))
            .collect();

        Self {
            results,
            by_run_id,
        }
    }

    /// Returns all results as a slice.
    pub fn all(&self) -> &[BacktestResult] {
        &self.results
    }

    /// Returns the number of results.
    pub fn len(&self) -> usize {
        self.results.len()
    }

    /// Returns true if there are no results.
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// Gets a result by RunId.
    pub fn get(&self, run_id: &str) -> Option<&BacktestResult> {
        self.by_run_id.get(run_id)
    }

    /// Returns results sorted by fitness (descending).
    pub fn sorted_by_fitness(&self) -> Vec<&BacktestResult> {
        let mut sorted: Vec<_> = self.results.iter().collect();
        sorted.sort_by(|a, b| {
            b.stats
                .fitness()
                .partial_cmp(&a.stats.fitness())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted
    }

    /// Returns the top N results by fitness.
    pub fn top_n(&self, n: usize) -> Vec<&BacktestResult> {
        let sorted = self.sorted_by_fitness();
        sorted.into_iter().take(n).collect()
    }

    /// Returns the best result by fitness.
    pub fn best(&self) -> Option<&BacktestResult> {
        self.sorted_by_fitness().into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::ResultCache;
    use crate::config::{
        ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, StrategyConfig,
    };
    use chrono::NaiveDate;

    fn make_base_config() -> RunConfig {
        RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::BuyAndHold,
                order_policy: OrderPolicyConfig::Simple,
                position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
            },
            start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2020, 3, 31).unwrap(),
            universe: vec!["SPY".to_string()],
            execution: ExecutionConfig::default(),
            initial_capital: 100_000.0,
        }
    }

    #[test]
    fn test_param_grid_size() {
        let grid = ParamGrid {
            ma_short_periods: vec![10, 20],
            ma_long_periods: vec![50, 100],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        };

        // 2 short × 2 long × 1 capital × 1 universe = 4 combinations
        assert_eq!(grid.size(), 4);
    }

    #[test]
    fn test_param_grid_filters_invalid_combinations() {
        let grid = ParamGrid {
            ma_short_periods: vec![10, 50, 100],
            ma_long_periods: vec![50, 100],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        };

        let base = make_base_config();
        let configs = grid.generate_configs(&base);

        // Valid combinations: (10,50), (10,100), (50,100)
        // Invalid: (50,50), (100,50), (100,100)
        assert_eq!(configs.len(), 3);

        // Verify all configs have short < long
        for config in &configs {
            if let SignalGeneratorConfig::MaCrossover {
                short_period,
                long_period,
            } = config.strategy.signal_generator
            {
                assert!(short_period < long_period);
            }
        }
    }

    #[test]
    fn test_param_sweep_sequential() {
        let runner = Runner::new();
        let sweep = ParamSweep::new(runner).with_parallelism(false);

        let grid = ParamGrid {
            ma_short_periods: vec![10],
            ma_long_periods: vec![50],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        };

        let base = make_base_config();
        let results = sweep.sweep(&grid, &base).unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_param_sweep_parallel() {
        let runner = Runner::new();
        let sweep = ParamSweep::new(runner).with_parallelism(true);

        let grid = ParamGrid {
            ma_short_periods: vec![10, 20],
            ma_long_periods: vec![50, 100],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        };

        let base = make_base_config();
        let results = sweep.sweep(&grid, &base).unwrap();

        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_sweep_results_sorted_by_fitness() {
        let runner = Runner::new();
        let sweep = ParamSweep::new(runner);

        let grid = ParamGrid {
            ma_short_periods: vec![10, 20],
            ma_long_periods: vec![50, 100],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        };

        let base = make_base_config();
        let results = sweep.sweep(&grid, &base).unwrap();

        let sorted = results.sorted_by_fitness();
        assert_eq!(sorted.len(), 4);

        // Verify sorted descending by fitness
        for i in 0..sorted.len() - 1 {
            assert!(sorted[i].stats.fitness() >= sorted[i + 1].stats.fitness());
        }
    }

    #[test]
    fn test_sweep_with_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = ResultCache::new(temp_dir.path()).unwrap();
        let runner = Runner::with_cache(cache.clone());
        let sweep = ParamSweep::new(runner);

        let grid = ParamGrid {
            ma_short_periods: vec![10],
            ma_long_periods: vec![50],
            initial_capitals: vec![100_000.0],
            universes: vec![vec!["SPY".to_string()]],
        };

        let base = make_base_config();

        // First sweep: compute and cache
        let results1 = sweep.sweep(&grid, &base).unwrap();
        assert_eq!(cache.len().unwrap(), 1);

        // Second sweep: should hit cache
        let results2 = sweep.sweep(&grid, &base).unwrap();
        assert_eq!(cache.len().unwrap(), 1);

        // Same results
        assert_eq!(results1.len(), results2.len());
    }
}
