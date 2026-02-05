//! BDD tests for M8: Runner & Sweeps
//!
//! These tests verify the Runner functionality:
//! - Single backtest execution
//! - Caching of results
//! - Parameter sweeps
//! - Leaderboard ranking

use chrono::NaiveDate;
use trendlab_runner::{
    BacktestResult, FitnessMetric, Leaderboard, ParamGrid, ParamSweep, ResultCache, RunConfig,
    Runner,
};

#[test]
fn bdd_scenario_run_single_backtest_with_ma_crossover() {
    // GIVEN a RunConfig with MA crossover strategy
    let config = create_ma_crossover_config(10, 50);

    // WHEN the runner executes the backtest
    let runner = Runner::new();
    let result = runner.run(&config).expect("Backtest should succeed");

    // THEN we should get a valid BacktestResult with equity curve and statistics
    assert_eq!(result.run_id, config.run_id());
    assert!(!result.equity_curve.is_empty());
    assert_eq!(result.stats.initial_equity, 100_000.0);
    assert!(result.stats.final_equity > 0.0);
}

#[test]
fn bdd_scenario_sweep_ma_periods_and_cache_results() {
    // GIVEN a parameter grid with multiple MA periods
    let grid = ParamGrid {
        ma_short_periods: vec![10, 20],
        ma_long_periods: vec![50, 100],
        initial_capitals: vec![100_000.0],
        universes: vec![vec!["SPY".to_string()]],
    };

    let base_config = create_base_config();

    // AND a runner with caching enabled
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = ResultCache::new(temp_dir.path()).unwrap();
    let runner = Runner::with_cache(cache.clone());
    let sweep = ParamSweep::new(runner);

    // WHEN we execute the parameter sweep
    let results = sweep
        .sweep(&grid, &base_config)
        .expect("Sweep should succeed");

    // THEN we should get results for all valid combinations
    // Valid: (10,50), (10,100), (20,50), (20,100) = 4 results
    assert_eq!(results.len(), 4);

    // AND all results should be cached
    assert_eq!(cache.len().unwrap(), 4);

    // WHEN we run the sweep again
    let results2 = sweep
        .sweep(&grid, &base_config)
        .expect("Second sweep should succeed");

    // THEN we should hit the cache and get the same results
    assert_eq!(results2.len(), 4);
    assert_eq!(cache.len().unwrap(), 4); // No new entries

    // AND the cache should contain matching run IDs
    for result in results.all() {
        assert!(cache.contains(&result.run_id));
    }
}

#[test]
fn bdd_scenario_generate_leaderboard_sorted_by_sharpe() {
    // GIVEN multiple backtest results with different Sharpe ratios
    let configs = vec![
        create_ma_crossover_config(10, 50),
        create_ma_crossover_config(20, 100),
        create_ma_crossover_config(15, 75),
    ];

    let runner = Runner::new();
    let results: Vec<BacktestResult> = configs
        .iter()
        .map(|c| runner.run(c).expect("Backtest should succeed"))
        .collect();

    // WHEN we create a leaderboard sorted by Sharpe ratio
    let leaderboard = Leaderboard::new(results).with_metric(FitnessMetric::Sharpe);

    // THEN the results should be sorted descending by Sharpe
    let sorted = leaderboard.sorted();
    assert_eq!(sorted.len(), 3);

    for i in 0..sorted.len() - 1 {
        assert!(sorted[i].stats.sharpe >= sorted[i + 1].stats.sharpe);
    }

    // AND we should be able to get the top N results
    let top_2 = leaderboard.top_n(2);
    assert_eq!(top_2.len(), 2);

    // AND the best result should have the highest Sharpe
    let best = leaderboard.best().unwrap();
    assert_eq!(best.stats.sharpe, sorted[0].stats.sharpe);
}

#[test]
fn bdd_scenario_cache_deduplication_by_run_id() {
    // GIVEN two identical configurations
    let config = create_ma_crossover_config(10, 50);

    // WHEN we run them both with caching
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = ResultCache::new(temp_dir.path()).unwrap();
    let runner = Runner::with_cache(cache.clone());

    let result1 = runner.run(&config).expect("First run should succeed");
    let result2 = runner.run(&config).expect("Second run should succeed");

    // THEN both results should have the same run_id
    assert_eq!(result1.run_id, result2.run_id);

    // AND the cache should contain only one entry
    assert_eq!(cache.len().unwrap(), 1);

    // AND the cached result should match
    let cached = cache.get(&result1.run_id).unwrap().unwrap();
    assert_eq!(cached.run_id, result1.run_id);
}

#[test]
fn bdd_scenario_filter_leaderboard_by_minimum_sharpe() {
    // GIVEN multiple backtest results
    let configs = vec![
        create_ma_crossover_config(10, 50),
        create_ma_crossover_config(20, 100),
        create_ma_crossover_config(15, 75),
    ];

    let runner = Runner::new();
    let results: Vec<BacktestResult> = configs
        .iter()
        .map(|c| runner.run(c).expect("Backtest should succeed"))
        .collect();

    let leaderboard = Leaderboard::new(results).with_metric(FitnessMetric::Sharpe);

    // WHEN we filter by minimum Sharpe of 0.0
    let filtered = leaderboard.filter_by_min_fitness(0.0);

    // THEN we should get results with Sharpe >= 0.0
    for result in &filtered {
        assert!(result.stats.sharpe >= 0.0);
    }

    // AND if we set a high threshold, we should get fewer results
    let min_sharpe = leaderboard.best().unwrap().stats.sharpe - 0.1;
    let filtered_high = leaderboard.filter_by_min_fitness(min_sharpe);

    assert!(filtered_high.len() <= filtered.len());
}

#[test]
fn bdd_scenario_parallel_sweep_produces_same_results_as_sequential() {
    // GIVEN a small parameter grid
    let grid = ParamGrid {
        ma_short_periods: vec![10, 20],
        ma_long_periods: vec![50],
        initial_capitals: vec![100_000.0],
        universes: vec![vec!["SPY".to_string()]],
    };

    let base_config = create_base_config();

    // WHEN we run sequential sweep
    let runner_seq = Runner::new();
    let sweep_seq = ParamSweep::new(runner_seq).with_parallelism(false);
    let results_seq = sweep_seq
        .sweep(&grid, &base_config)
        .expect("Sequential sweep should succeed");

    // AND we run parallel sweep
    let runner_par = Runner::new();
    let sweep_par = ParamSweep::new(runner_par).with_parallelism(true);
    let results_par = sweep_par
        .sweep(&grid, &base_config)
        .expect("Parallel sweep should succeed");

    // THEN both should produce the same number of results
    assert_eq!(results_seq.len(), results_par.len());

    // AND all run IDs should match (order-independent)
    let seq_ids: std::collections::HashSet<_> =
        results_seq.all().iter().map(|r| &r.run_id).collect();
    let par_ids: std::collections::HashSet<_> =
        results_par.all().iter().map(|r| &r.run_id).collect();

    assert_eq!(seq_ids, par_ids);
}

// Helper functions

fn create_base_config() -> RunConfig {
    use trendlab_runner::{
        ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, SignalGeneratorConfig,
        StrategyConfig,
    };

    RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::BuyAndHold,
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
        },
        start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2020, 6, 30).unwrap(),
        universe: vec!["SPY".to_string()],
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    }
}

fn create_ma_crossover_config(short_period: usize, long_period: usize) -> RunConfig {
    use trendlab_runner::{
        ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, SignalGeneratorConfig,
        StrategyConfig,
    };

    RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period,
                long_period,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedDollar { amount: 10000.0 },
        },
        start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2020, 6, 30).unwrap(),
        universe: vec!["SPY".to_string()],
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    }
}
