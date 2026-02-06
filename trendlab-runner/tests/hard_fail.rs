use chrono::NaiveDate;
use trendlab_runner::{
    ExecutionConfig, OrderPolicyConfig, ParamGrid, ParamSweep, PositionSizerConfig, ResultCache,
    RunConfig, Runner, SignalGeneratorConfig, StrategyConfig,
};

fn base_config() -> RunConfig {
    RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
                long_period: 50,
            },
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
fn hard_fail_concurrency_torture() {
    let grid = ParamGrid {
        ma_short_periods: vec![10, 20],
        ma_long_periods: vec![50, 100],
        initial_capitals: vec![100_000.0],
        universes: vec![vec!["SPY".to_string()]],
    };

    let base = base_config();
    let results_serial = ParamSweep::new(Runner::new())
        .with_parallelism(false)
        .sweep(&grid, &base)
        .unwrap();
    let results_parallel = ParamSweep::new(Runner::new())
        .with_parallelism(true)
        .sweep(&grid, &base)
        .unwrap();

    assert_eq!(results_serial.len(), results_parallel.len());

    let serial_sorted = results_serial.sorted_by_fitness();
    let parallel_sorted = results_parallel.sorted_by_fitness();
    for (left, right) in serial_sorted.iter().zip(parallel_sorted.iter()) {
        assert_eq!(left.run_id, right.run_id);
        assert_eq!(left.equity_curve, right.equity_curve);
        assert_eq!(left.trades, right.trades);
    }
}

#[test]
fn hard_fail_cache_mutation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = ResultCache::new(temp_dir.path()).unwrap();
    let runner = Runner::with_cache(cache.clone());

    let config = base_config();
    let result1 = runner.run(&config).unwrap();

    let cache_file = temp_dir.path().join(format!("{}.json", result1.run_id));
    std::fs::remove_file(&cache_file).unwrap();

    let result2 = runner.run(&config).unwrap();
    assert_eq!(result1.equity_curve, result2.equity_curve);
    assert_eq!(result1.trades, result2.trades);
}
